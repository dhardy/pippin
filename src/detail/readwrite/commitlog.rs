/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Pippin change/commit log reading and writing

//! Support for reading and writing Rust snapshots

use std::io::{Read, Write};
use std::collections::HashMap;
use std::rc::Rc;
use std::u32;

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use detail::readwrite::{sum};
use detail::{Commit, EltChange, CommitMeta};
use {ElementT, Sum};
use detail::SUM_BYTES;
use error::{Result, ReadError};

/// Implement this to use read_log().
pub trait CommitReceiver<E: ElementT> {
    /// Implement to receive a commit once it has been read. Return true to
    /// continue reading or false to stop reading more commits.
    fn receive(&mut self, commit: Commit<E>) -> bool;
}

/// Read a commit log from a stream
pub fn read_log<E: ElementT>(reader_: &mut Read, receiver: &mut CommitReceiver<E>) -> Result<()> {
    let mut reader = reader_;
    let mut pos: usize = 0;
    let mut buf = vec![0; 32];
    
    try!(reader.read_exact(&mut buf[0..16]));
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
        
        let l = try!(r.read(&mut buf[0..16]));
        if l == 0 { break; /*end of file (EOF)*/ }
        if l < 16 { try!(r.read_exact(&mut buf[l..16])); /*not EOF, buf haven't filled buffer*/ }
        
        let n_parents = if buf[0..6] == *b"COMMIT" {
            1
        } else if buf[0..5] == *b"MERGE" {
            let n: u8 = buf[5];
            if n < 2 { return ReadError::err("bad number of parents", pos, (5, 6)); }
            n as usize
        } else {
            return ReadError::err("unexpected contents (expected COMMIT or MERGE)", pos, (0, 6));
        };
        let meta = if buf[6..8] == *b"\x00\x00" {
            // Compatibility mode (2016_02_01 and older): no timestamp etc.
            pos += 16;
            CommitMeta {
                number: 1,
                timestamp: 0,
                extra: None
            }
        } else if buf[6..8] == *b"\x00U" {
            let secs = try!((&buf[8..16]).read_i64::<BigEndian>());
            pos += 16;
            
            try!(r.read_exact(&mut buf[0..16]));
            if buf[0..4] != *b"CNUM" {
                return ReadError::err("unexpected contents (expected CNUM)", pos, (0, 4));
            }
            let cnum = try!((&buf[4..8]).read_u32::<BigEndian>());
            
            if buf[8..10] != *b"XM" {
                return ReadError::err("unexpected contents (expected XM)", pos, (8, 10));
            }
            let xm_type_txt = buf[10..12] == *b"TT";
            let xm_len = try!((&buf[12..16]).read_u32::<BigEndian>()) as usize;
            pos += 16;
            
            let mut xm_data = vec![0; xm_len];
            try!(r.read_exact(&mut xm_data));
            let xm = if xm_type_txt {
                Some(try!(String::from_utf8(xm_data)
                    .map_err(|_| ReadError::new("content not valid UTF-8", pos, (0, xm_len)))))
            } else {
                // even if xm_len > 0 we ignore it
                None
            };
            
            pos += xm_len;
            let pad_len = 16 * ((xm_len + 15) / 16) - xm_len;
            if pad_len > 0 {
                try!(r.read_exact(&mut buf[0..pad_len]));
                pos += pad_len;
            }
            
            CommitMeta {
                number: cnum,
                timestamp: secs,
                extra: xm,
            }
        } else {
            return ReadError::err("unexpected contents (expected \\x00U or \\x00\\x00)", pos, (6, 8));
        };
        
        let mut parents = Vec::with_capacity(n_parents);
        for _ in 0..n_parents {
            try!(r.read_exact(&mut buf[0..SUM_BYTES]));
            parents.push(Sum::load(&buf[0..SUM_BYTES]));
            pos += SUM_BYTES;
        }
        
        try!(r.read_exact(&mut buf[0..16]));
        if buf[0..8] != *b"ELEMENTS" {
            return ReadError::err("unexpected contents (expected ELEMENTS)", pos, (0, 8));
        }
        let num_elts = try!((&buf[8..16]).read_u64::<BigEndian>()) as usize;   // #0015
        pos += 16;
        
        let mut changes = HashMap::new();
        
        for _ in 0..num_elts {
            try!(r.read_exact(&mut buf[0..16]));
            if buf[0..4] != *b"ELT " {
                return ReadError::err("unexpected contents (expected ELT\\x20)", pos, (0, 4));
            }
            let elt_id = try!((&buf[8..16]).read_u64::<BigEndian>()).into();
            let change_t = match &buf[4..8] {
                b"DEL\x00" => { Change::Delete },
                b"INS\x00" => { Change::Insert },
                b"REPL" => { Change::Replace },
                b"MOVO" => { Change::MovedOut },
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
                    try!(r.read_exact(&mut buf[0..16]));
                    if buf[0..8] != *b"ELT DATA" {
                        return ReadError::err("unexpected contents (expected ELT DATA)", pos, (0, 8));
                    }
                    let data_len = try!((&buf[8..16]).read_u64::<BigEndian>()) as usize;   // #0015
                    pos += 16;
                    
                    let mut data = vec![0; data_len];
                    try!(r.read_exact(&mut data));
                    pos += data_len;
                    
                    let pad_len = 16 * ((data_len + 15) / 16) - data_len;
                    if pad_len > 0 {
                        try!(r.read_exact(&mut buf[0..pad_len]));
                        pos += pad_len;
                    }
                    
                    let data_sum = Sum::calculate(&data);
                    try!(r.read_exact(&mut buf[0..SUM_BYTES]));
                    if !data_sum.eq(&buf[0..SUM_BYTES]) {
                        return ReadError::err("element checksum mismatch", pos, (0, SUM_BYTES));
                    }
                    pos += SUM_BYTES;
                    
                    let elt = Rc::new(try!(E::from_vec(data)));
                    match change_t {
                        Change::Insert => EltChange::insertion(elt),
                        Change::Replace => EltChange::replacement(elt),
                        _ => panic!()
                    }
                },
                Change::MovedOut | Change::Moved => {
                    try!(r.read_exact(&mut buf[0..16]));
                    if buf[0..8] != *b"NEW ELT\x00" {
                        return ReadError::err("unexpected contents (expected NEW ELT)", pos, (0, 8));
                    }
                    let new_id = try!((&buf[8..16]).read_u64::<BigEndian>()).into();
                    EltChange::moved(new_id, change_t == Change::MovedOut)
                }
            };
            changes.insert(elt_id, change);
        }
        
        try!(r.read_exact(&mut buf[0..SUM_BYTES]));
        let commit_sum = Sum::load(&buf[0..SUM_BYTES]);
        pos += SUM_BYTES;
        
        let sum = r.sum();
        reader = r.into_inner();
        try!(reader.read_exact(&mut buf[0..SUM_BYTES]));
        if !sum.eq(&buf[0..SUM_BYTES]) {
            return ReadError::err("checksum invalid", pos, (0, SUM_BYTES));
        }
        
        trace!("Read commit ({} changes): {}; first parent: {}", changes.len(), commit_sum, parents[0]);
        let cont = receiver.receive(Commit::new(commit_sum, parents, changes, meta));
        if !cont { break; }
    }
    
    #[derive(Eq, PartialEq, Copy, Clone, Debug)]
    enum Change {
        Delete, Insert, Replace, MovedOut, Moved
    }
    
    Ok(())
}

/// Write the section identifier at the start of a commit log
// #0016: do we actually need this?
pub fn start_log(writer: &mut Write) -> Result<()> {
    try!(writer.write(b"COMMIT LOG\x00\x00\x00\x00\x00\x00"));
    Ok(())
}

/// Write a single commit to a stream
pub fn write_commit<E: ElementT>(commit: &Commit<E>, writer: &mut Write) -> Result<()> {
    trace!("Writing commit ({} changes): {}",
        commit.num_changes(), commit.statesum());
    
    // A writer which calculates the checksum of what was written:
    let mut w = sum::HashWriter::new(writer);
    
    if commit.parents().len() == 1 {
        try!(w.write(b"COMMIT\x00U"));
    } else {
        assert!(commit.parents().len() > 1 && commit.parents().len() < 0x100);
        try!(w.write(b"MERGE"));
        let n: [u8; 1] = [commit.parents().len() as u8];
        try!(w.write(&n));
        try!(w.write(b"\x00U"));
    }
    
    try!(w.write_i64::<BigEndian>(commit.meta().timestamp));
    
    try!(w.write(b"CNUM"));
    try!(w.write_u32::<BigEndian>(commit.meta().number));
    
    if let Some(ref txt) = commit.meta().extra {
        try!(w.write(b"XMTT"));
        assert!(txt.len() <= u32::MAX as usize);
        try!(w.write_u32::<BigEndian>(txt.len() as u32));
        try!(w.write(txt.as_bytes()));
        let pad_len = 16 * ((txt.len() + 15) / 16) - txt.len();
        if pad_len > 0 {
            let padding = [0u8; 15];
            try!(w.write(&padding[0..pad_len]));
        }
    } else {
        // last four zeros is 0u32 encoded in bytes
        try!(w.write(b"XM\x00\x00\x00\x00\x00\x00"));
    }
    
    // Parent statesums (we wrote the number above already):
    for parent in commit.parents() {
        try!(parent.write(&mut w));
    }
    
    try!(w.write(b"ELEMENTS"));
    try!(w.write_u64::<BigEndian>(commit.num_changes() as u64));       // #0015
    
    let mut elt_buf = Vec::new();
    
    for (elt_id,change) in commit.changes_iter() {
        let marker = match change {
            &EltChange::Deletion => b"ELT DEL\x00",
            &EltChange::Insertion(_) => b"ELT INS\x00",
            &EltChange::Replacement(_) => b"ELT REPL",
            &EltChange::MovedOut(_) => b"ELT MOVO",
            &EltChange::Moved(_) => b"ELT MOV\x00",
        };
        try!(w.write(marker));
        try!(w.write_u64::<BigEndian>((*elt_id).into()));
        if let Some(elt) = change.element() {
            try!(w.write(b"ELT DATA"));
            elt_buf.clear();
            try!(elt.write_buf(&mut &mut elt_buf));
            try!(w.write_u64::<BigEndian>(elt_buf.len() as u64));      // #0015
            
            try!(w.write(&elt_buf));
            let pad_len = 16 * ((elt_buf.len() + 15) / 16) - elt_buf.len();
            if pad_len > 0 {
                let padding = [0u8; 15];
                try!(w.write(&padding[0..pad_len]));
            }
            
            try!(elt.sum().write(&mut w));
        }
        if let Some(new_id) = change.moved_id() {
            try!(w.write(b"NEW ELT\x00"));
            try!(w.write_u64::<BigEndian>(new_id.into()));
        }
    }
    
    try!(commit.statesum().write(&mut w));
    
    let sum = w.sum();
    try!(sum.write(&mut w.into_inner()));
    
    Ok(())
}

#[test]
fn commit_write_read(){
    use PartId;
    
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
    let meta1 = CommitMeta { number: 1, timestamp: 123456, extra: None };
    let commit_1 = Commit::new(seq, vec![squares], changes, meta1);
    
    changes = HashMap::new();
    changes.insert(p.elt_id(1), EltChange::deletion());
    changes.insert(p.elt_id(9), EltChange::replacement(Rc::new("NINE!".to_string())));
    changes.insert(p.elt_id(5), EltChange::insertion(Rc::new("five again?".to_string())));
    let meta2 = CommitMeta { number: 1, timestamp: 321654, extra: Some("123".to_string()) };
    let commit_2 = Commit::new(nonsense, vec![quadr], changes, meta2);
    
    let mut obj = Vec::new();
    assert!(start_log(&mut obj).is_ok());
    assert!(write_commit(&commit_1, &mut obj).is_ok());
    assert!(write_commit(&commit_2, &mut obj).is_ok());
    
    impl CommitReceiver<String> for Vec<Commit<String>> {
        fn receive(&mut self, commit: Commit<String>) -> bool { self.push(commit); true }
    }
    let mut commits = Vec::new();
    match read_log(&mut &obj[..], &mut commits) {
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
