/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Pippin change/commit log reading and writing

//! Support for reading and writing Rust snapshots

use std::io::{Read, Write};
use std::collections::HashMap;
use std::rc::Rc;
use std::u32;

use byteorder::{ByteOrder, BigEndian, WriteBytesExt};

use readwrite::{sum, read_meta, write_meta};
use commit::{Commit, EltChange};
use elt::ElementT;
use sum::{Sum, SUM_BYTES};
use error::{Result, ReadError};

/// Implement this to use `read_log()`.
/// 
/// There is a simple implementation for `Vec<Commit<E>>` which just pushes
/// each commit and returns `true` (to continue reading to the end).
pub trait CommitReceiver<E: ElementT> {
    /// Implement to receive a commit once it has been read. Return true to
    /// continue reading or false to stop reading more commits.
    fn receive(&mut self, commit: Commit<E>) -> bool;
}
impl<E: ElementT> CommitReceiver<E> for Vec<Commit<E>> {
    /// Implement function required by readwrite::read_log().
    fn receive(&mut self, commit: Commit<E>) -> bool {
        self.push(commit);
        true    // continue reading to EOF
    }
}


/// Read a commit log from a stream
/// 
/// `format_ver` is the decimalised file format version
pub fn read_log<E: ElementT>(mut reader: &mut Read,
        receiver: &mut CommitReceiver<E>, format_ver: u32) -> Result<()>
{
    let mut pos: usize = 0;
    let mut buf = vec![0; 32];
    
    reader.read_exact(&mut buf[0..16])?;
    if buf[0..16] != *b"COMMIT LOG\x00\x00\x00\x00\x00\x00" {
        return ReadError::err("unexpected contents (expected \
            COMMIT LOG\\x00\\x00\\x00\\x00\\x00\\x00)", pos, (0, 16));
    }
    pos += 16;
    
    // We now read commits. Since new commits can simply be appended to the
    // file, we only know we're at the end if we hit EOF. This is the only
    // condition where encountering EOF is not an error.
    loop {
        // A reader which calculates the checksum of what was read:
        let mut r = sum::HashReader::new(reader);
        
        let l = r.read(&mut buf[0..16])?;
        if l == 0 { break; /*end of file (EOF)*/ }
        if l < 16 { r.read_exact(&mut buf[l..16])?; /*not EOF, buf haven't filled buffer*/ }
        
        let n_parents = if buf[0..6] == *b"COMMIT" {
            1
        } else if buf[0..5] == *b"MERGE" {
            let n: u8 = buf[5];
            if n < 2 { return ReadError::err("bad number of parents", pos, (5, 6)); }
            n as usize
        } else {
            return ReadError::err("unexpected contents (expected COMMIT or MERGE)", pos, (0, 6));
        };
        if buf[6..8] != *b"\x00U" {
            return ReadError::err("unexpected contents (expected \\x00U)", pos, (6, 8));
        }
        let meta = read_meta(&mut r, &mut buf, &mut pos, format_ver)?;
        
        let mut parents = Vec::with_capacity(n_parents);
        for _ in 0..n_parents {
            r.read_exact(&mut buf[0..SUM_BYTES])?;
            parents.push(Sum::load(&buf[0..SUM_BYTES]));
            pos += SUM_BYTES;
        }
        
        r.read_exact(&mut buf[0..16])?;
        if buf[0..8] != *b"ELEMENTS" {
            return ReadError::err("unexpected contents (expected ELEMENTS)", pos, (0, 8));
        }
        let num_elts = BigEndian::read_u64(&buf[8..16]) as usize;   // #0015
        pos += 16;
        
        let mut changes = HashMap::new();
        
        for _ in 0..num_elts {
            r.read_exact(&mut buf[0..16])?;
            if buf[0..4] != *b"ELT " {
                return ReadError::err("unexpected contents (expected ELT\\x20)", pos, (0, 4));
            }
            let elt_id = BigEndian::read_u64(&buf[8..16]).into();
            let change_t = match &buf[4..8] {
                b"DEL\x00" => { Change::Delete },
                b"INS\x00" => { Change::Insert },
                b"REPL" => { Change::Replace },
                b"MOVO" => { Change::MoveOut },
                b"MOV\x00" => { Change::Moved },
                _ => {
                    return ReadError::err("unexpected contents (expected one \
                        of DEL\\x00, INS\\x00, REPL)", pos, (4, 8));
                }
            };
            pos += 16;
            
            let change = match change_t {
                Change::Delete => EltChange::deletion(),
                Change::Insert | Change::Replace => {
                    r.read_exact(&mut buf[0..16])?;
                    if buf[0..8] != *b"ELT DATA" {
                        return ReadError::err("unexpected contents (expected ELT DATA)", pos, (0, 8));
                    }
                    let data_len = BigEndian::read_u64(&buf[8..16]) as usize;   // #0015
                    pos += 16;
                    
                    let mut data = vec![0; data_len];
                    r.read_exact(&mut data)?;
                    pos += data_len;
                    
                    let pad_len = 16 * ((data_len + 15) / 16) - data_len;
                    if pad_len > 0 {
                        r.read_exact(&mut buf[0..pad_len])?;
                        pos += pad_len;
                    }
                    
                    let elt_sum = Sum::elt_sum(elt_id, &data);
                    r.read_exact(&mut buf[0..SUM_BYTES])?;
                    if elt_sum != buf[0..SUM_BYTES] {
                        return ReadError::err("element checksum mismatch", pos, (0, SUM_BYTES));
                    }
                    pos += SUM_BYTES;
                    
                    let elt = Rc::new(E::from_vec_sum(data, elt_sum)?);
                    match change_t {
                        Change::Insert => EltChange::insertion(elt),
                        Change::Replace => EltChange::replacement(elt),
                        _ => panic!()
                    }
                },
                Change::MoveOut | Change::Moved => {
                    r.read_exact(&mut buf[0..16])?;
                    if buf[0..8] != *b"NEW ELT\x00" {
                        return ReadError::err("unexpected contents (expected NEW ELT)", pos, (0, 8));
                    }
                    let new_id = BigEndian::read_u64(&buf[8..16]).into();
                    EltChange::moved(new_id, change_t == Change::MoveOut)
                }
            };
            changes.insert(elt_id, change);
        }
        
        r.read_exact(&mut buf[0..SUM_BYTES])?;
        let commit_sum = Sum::load(&buf[0..SUM_BYTES]);
        pos += SUM_BYTES;
        
        let sum = r.sum();
        reader = r.into_inner();
        reader.read_exact(&mut buf[0..SUM_BYTES])?;
        if sum != buf[0..SUM_BYTES] {
            return ReadError::err("checksum invalid", pos, (0, SUM_BYTES));
        }
        
        trace!("Read commit ({} changes): {}; first parent: {}", changes.len(), commit_sum, parents[0]);
        let cont = receiver.receive(Commit::new_explicit(commit_sum, parents, changes, meta));
        if !cont { break; }
    }
    
    #[derive(Eq, PartialEq, Copy, Clone, Debug)]
    enum Change {
        Delete, Insert, Replace, MoveOut, Moved
    }
    
    Ok(())
}

/// Write the section identifier at the start of a commit log
// #0016: do we actually need this?
pub fn start_log(writer: &mut Write) -> Result<()> {
    writer.write_all(b"COMMIT LOG\x00\x00\x00\x00\x00\x00")?;
    Ok(())
}

/// Write a single commit to a stream
pub fn write_commit<E: ElementT>(commit: &Commit<E>, writer: &mut Write) -> Result<()> {
    trace!("Writing commit ({} changes): {}",
        commit.num_changes(), commit.statesum());
    
    // A writer which calculates the checksum of what was written:
    let mut w = sum::HashWriter::new(writer);
    
    if commit.parents().len() == 1 {
        w.write_all(b"COMMIT\x00U")?;
    } else {
        assert!(commit.parents().len() > 1 && commit.parents().len() < 0x100);
        w.write_all(b"MERGE")?;
        let n: [u8; 1] = [commit.parents().len() as u8];
        w.write_all(&n)?;
        w.write_all(b"\x00U")?;
    }
    
    write_meta(&mut w, commit.meta())?;
    
    // Parent statesums (we wrote the number above already):
    for parent in commit.parents() {
        parent.write_to(&mut w)?;
    }
    
    w.write_all(b"ELEMENTS")?;
    w.write_u64::<BigEndian>(commit.num_changes() as u64)?;       // #0015
    
    let mut elt_buf = Vec::new();
    
    let mut keys: Vec<_> = commit.changes_iter().map(|(k,_)| k).collect();
    keys.sort();
    for elt_id in keys {
        let change = commit.change(*elt_id).expect("get change");
        let marker = match *change {
            EltChange::Deletion => b"ELT DEL\x00",
            EltChange::Insertion(_) => b"ELT INS\x00",
            EltChange::Replacement(_) => b"ELT REPL",
            EltChange::MoveOut(_) => b"ELT MOVO",
            EltChange::Moved(_) => b"ELT MOV\x00",
        };
        w.write_all(marker)?;
        w.write_u64::<BigEndian>((*elt_id).into())?;
        if let Some(elt) = change.element() {
            w.write_all(b"ELT DATA")?;
            elt_buf.clear();
            elt.write_buf(&mut &mut elt_buf)?;
            w.write_u64::<BigEndian>(elt_buf.len() as u64)?;      // #0015
            
            w.write_all(&elt_buf)?;
            let pad_len = 16 * ((elt_buf.len() + 15) / 16) - elt_buf.len();
            if pad_len > 0 {
                let padding = [0u8; 15];
                w.write_all(&padding[0..pad_len])?;
            }
            
            elt.sum(*elt_id).write_to(&mut w)?;
        }
        if let Some(new_id) = change.moved_id() {
            w.write_all(b"NEW ELT\x00")?;
            w.write_u64::<BigEndian>(new_id.into())?;
        }
    }
    
    commit.statesum().write_to(&mut w)?;
    
    let sum = w.sum();
    sum.write_to(&mut w.into_inner())?;
    
    Ok(())
}

#[test]
fn commit_write_read(){
    use readwrite::header::HEAD_VERSIONS;
    use commit::{CommitMeta, ExtraMeta, MetaFlags};
    use elt::PartId;
    
    // Note that we can make up completely nonsense commits here. Element
    // checksums must still match but state sums don't need to since we won't
    // be reproducing states. So lets make some fun sums!
    let mut v: Vec<u8> = (0u8..).take(SUM_BYTES).collect();
    let seq = Sum::load(&v);
    v = (0u8..).map(|x| x.wrapping_mul(x)).take(SUM_BYTES).collect();
    let squares = Sum::load(&v);
    v = (1u8..).map(|x| x.wrapping_add(7u8).wrapping_mul(3u8)).take(SUM_BYTES).collect();
    let nonsense = Sum::load(&v);
    v = (1u8..).map(|x| x.wrapping_mul(x).wrapping_add(5u8.wrapping_mul(x)).wrapping_add(11u8)).take(SUM_BYTES).collect();
    let quadr = Sum::load(&v);
    
    let p = PartId::from_num(1681);
    let mut changes = HashMap::new();
    changes.insert(p.elt_id(3), EltChange::insertion(Rc::new("three".to_string())));
    changes.insert(p.elt_id(4), EltChange::insertion(Rc::new("four".to_string())));
    changes.insert(p.elt_id(5), EltChange::insertion(Rc::new("five".to_string())));
    let meta1 = CommitMeta::new_explicit(1, 123456, MetaFlags::zero(), vec![], ExtraMeta::None).expect("new meta");
    let commit_1 = Commit::new_explicit(seq, vec![squares], changes, meta1);
    
    changes = HashMap::new();
    changes.insert(p.elt_id(1), EltChange::deletion());
    changes.insert(p.elt_id(9), EltChange::replacement(Rc::new("NINE!".to_string())));
    changes.insert(p.elt_id(5), EltChange::insertion(Rc::new("five again?".to_string())));
    let meta2 = CommitMeta::new_explicit(1, 321654, MetaFlags::zero(), vec![], ExtraMeta::Text("123".to_string())).expect("new meta");
    let commit_2 = Commit::new_explicit(nonsense, vec![quadr], changes, meta2);
    
    let mut obj = Vec::new();
    assert!(start_log(&mut obj).is_ok());
    assert!(write_commit(&commit_1, &mut obj).is_ok());
    assert!(write_commit(&commit_2, &mut obj).is_ok());
    
    let mut commits = Vec::new();
    match read_log(&mut &obj[..], &mut commits, HEAD_VERSIONS[HEAD_VERSIONS.len() - 1]) {
        Ok(()) => {},
        Err(e) => {
//             // specialisation for a ReadError:
//             panic!("read_log failed: {}", e.display(&obj));
            panic!("read_log failed: {}", e);
        }
    }
    
    assert_eq!(commits.len(), 2);
    assert_eq!(commits[0], commit_1);
    assert_eq!(commits[1], commit_2);
}
