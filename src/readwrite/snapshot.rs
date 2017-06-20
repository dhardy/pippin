/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Support for reading and writing Rust snapshots

use std::io::{Read, Write};
use std::rc::Rc;
use std::{u8, u32};
use std::collections::hash_map::{HashMap, Entry};

use byteorder::{ByteOrder, BigEndian, WriteBytesExt};

use classify::Classification;
use readwrite::{sum, read_meta, write_meta};
use state::{PartState, StateRead};
use elt::{Element, PartId};
use sum::{Sum, SUM_BYTES};
use error::{Result, ReadError, ElementOp};

/// Read a snapshot of a set of elements from a stream.
/// 
/// This function reads to the end of the snapshot. It does not check whether
/// this is in fact the end of the file (or other data stream), though
/// according to the specified file format this should be the case.
/// 
/// The `part_id` parameter is assigned to the `PartState` returned.
/// 
/// The file version affects how data is read. Get it from a header with
/// `header.ftype.ver()`.
pub fn read_snapshot<T: Element>(reader: &mut Read, part_id: PartId,
        csf: Classification, format_ver: u32) -> Result<PartState<T>>
{
    // A reader which calculates the checksum of what was read:
    let mut r = sum::HashReader::new(reader);
    
    let mut pos: usize = 0;
    let mut buf = vec![0; 32];
    assert!(buf.len() >= SUM_BYTES);
    
    r.read_exact(&mut buf[0..16])?;
    if buf[0..6] != *b"SNAPSH" || buf[7] != b'U' {
        return ReadError::err("unexpected contents (expected SNAPSH_U where _ is any)", pos, (0, 8));
    }
    let num_parents = buf[6] as usize;
    let meta = read_meta(&mut r, &mut buf, &mut pos, format_ver)?;
    
    let mut parents = Vec::with_capacity(num_parents);
    for _ in 0..num_parents {
        r.read_exact(&mut buf[0..SUM_BYTES])?;
        parents.push(Sum::load(&buf[0..SUM_BYTES]));
    }
    
    r.read_exact(&mut buf[0..16])?;
    if buf[0..8] != *b"ELEMENTS" {
        return ReadError::err("unexpected contents (expected ELEMENTS)", pos, (0, 8));
    }
    let num_elts = BigEndian::read_u64(&buf[8..16]) as usize;    // #0015
    pos += 16;
    
    let mut elts = HashMap::new();
    let mut combined_elt_sum = Sum::zero();
    for _ in 0..num_elts {
        r.read_exact(&mut buf[0..32])?;
        if buf[0..8] != *b"ELEMENT\x00" {
            println!("buf: \"{}\", {:?}", String::from_utf8_lossy(&buf[0..8]), &buf[0..8]);
            return ReadError::err("unexpected contents (expected ELEMENT\\x00)", pos, (0, 8));
        }
        let ident = BigEndian::read_u64(&buf[8..16]).into();
        pos += 16;
        
        if buf[16..24] != *b"BYTES\x00\x00\x00" {
            return ReadError::err("unexpected contents (expected BYTES\\x00\\x00\\x00)", pos, (16, 24));
        }
        let data_len = BigEndian::read_u64(&buf[24..32]) as usize;   // #0015
        pos += 16;
        
        let mut data = vec![0; data_len];
        r.read_exact(&mut data)?;
        pos += data_len;
        
        let pad_len = 16 * ((data_len + 15) / 16) - data_len;
        if pad_len > 0 {
            r.read_exact(&mut buf[0..pad_len])?;
            pos += pad_len;
        }
        
        let elt_sum = Sum::elt_sum(ident, &data);
        r.read_exact(&mut buf[0..SUM_BYTES])?;
        if elt_sum != buf[0..SUM_BYTES] {
            return ReadError::err("element checksum mismatch", pos, (0, SUM_BYTES));
        }
        pos += SUM_BYTES;
        
        combined_elt_sum.permute(&elt_sum);
        
        let elt = T::from_vec_sum(data, elt_sum)?;
        if ident.part_id() != part_id { return Err(Box::new(ElementOp::WrongPartId)); }
        match elts.entry(ident) {
            Entry::Occupied(_) => { return Err(Box::new(ElementOp::IdClash)); },
            Entry::Vacant(e) => e.insert(Rc::new(elt)),
        };
    }
    
    let mut moves = HashMap::new();
    r.read_exact(&mut buf[0..16])?;
    if buf[0..8] == *b"ELTMOVES" /*versions from 20160201, optional*/ {
        let n_moves = BigEndian::read_u64(&buf[8..16]) as usize;    // #0015
        for _ in 0..n_moves {
            r.read_exact(&mut buf[0..16])?;
            let id0 = BigEndian::read_u64(&buf[0..8]).into();
            let id1 = BigEndian::read_u64(&buf[8..16]).into();
            moves.insert(id0, id1);
        }
        // re-fill buffer for next section:
        r.read_exact(&mut buf[0..16])?;
    }
    
    let state = PartState::new_explicit(part_id, csf, parents,
            elts, moves, meta, combined_elt_sum);
    
    if buf[0..8] != *b"STATESUM" {
        return ReadError::err("unexpected contents (expected STATESUM or ELTMOVES)", pos, (0, 8));
    }
    pos += 8;
    if (BigEndian::read_u64(&buf[8..16]) as usize) != num_elts {
        return ReadError::err("unexpected contents (number of elements \
            differs from that previously stated)", pos, (8, 16));
    }
    pos += 8;
    
    r.read_exact(&mut buf[0..SUM_BYTES])?;
    if *state.statesum() != buf[0..SUM_BYTES] {
        return ReadError::err("state checksum mismatch", pos, (0, SUM_BYTES));
    }
    pos += SUM_BYTES;
    
    let sum = r.sum();
    let mut r = r.into_inner();
    r.read_exact(&mut buf[0..SUM_BYTES])?;
    if sum != buf[0..SUM_BYTES] {
        return ReadError::err("checksum invalid", pos, (0, SUM_BYTES));
    }
    
    trace!("Read snapshot (partition {} with {} elements): {}",
        part_id, num_elts, state.statesum());
    Ok(state)
}

/// Write a snapshot of a set of elements to a stream
/// 
/// The snapshot is derived from a partition state, but also includes a
/// partition identifier range.
pub fn write_snapshot<T: Element>(state: &PartState<T>,
    writer: &mut Write) -> Result<()>
{
    trace!("Writing snapshot (partition {} with {} elements): {}",
        state.part_id(), state.num_avail(), state.statesum());
    
    // A writer which calculates the checksum of what was written:
    let mut w = sum::HashWriter::new(writer);
    
    let mut snapsh_u: [u8; 8] = *b"SNAPSH_U";
    assert!(state.parents().len() <= (u8::MAX as usize));
    snapsh_u[6] = state.parents().len() as u8;
    w.write_all(&snapsh_u)?;
    write_meta(&mut w, state.meta())?;
    
    for parent in state.parents() {
        parent.write_to(&mut w)?;
    }
    
    w.write_all(b"ELEMENTS")?;
    let num_elts = state.elts_len() as u64;  // #0015
    w.write_u64::<BigEndian>(num_elts)?;
    
    let mut elt_buf = Vec::new();
    
    let mut keys: Vec<_> = state.elts_iter().map(|(k,_)| k).collect();
    keys.sort();
    for ident in keys {
        w.write_all(b"ELEMENT\x00")?;
        w.write_u64::<BigEndian>(ident.into())?;
        
        let elt = state.get_rc(ident).expect("get elt by key");
        w.write_all(b"BYTES\x00\x00\x00")?;
        elt_buf.clear();
        elt.write_buf(&mut &mut elt_buf)?;
        w.write_u64::<BigEndian>(elt_buf.len() as u64 /* #0015 */)?;
        
        w.write_all(&elt_buf)?;
        let pad_len = 16 * ((elt_buf.len() + 15) / 16) - elt_buf.len();
        if pad_len > 0 {
            let padding = [0u8; 15];
            w.write_all(&padding[0..pad_len])?;
        }
        
        elt.sum(ident).write_to(&mut w)?;
    }
    
    if state.moved_len() > 0 {
        w.write_all(b"ELTMOVES")?;
        w.write_u64::<BigEndian>(state.moved_len() as u64 /* #0015 */)?;
        for (ident, new_ident) in state.moved_iter() {
            w.write_u64::<BigEndian>(ident.into())?;
            w.write_u64::<BigEndian>(new_ident.into())?;
        }
    }
    
    // We write the checksum we kept in memory, the idea being that in-memory
    // corruption will be detected on next load.
    w.write_all(b"STATESUM")?;
    w.write_u64::<BigEndian>(num_elts)?;
    state.statesum().write_to(&mut w)?;
    
    // Write the checksum of everything above:
    let sum = w.sum();
    sum.write_to(&mut w.into_inner())?;
    
    Ok(())
}

#[test]
fn snapshot_writing() {
    use state::StateWrite;
    use readwrite::header::HEAD_VERSIONS;
    use commit::{CommitMeta, ExtraMeta, MakeCommitMeta};
    
    struct MMNone {}
    impl MakeCommitMeta for MMNone {
        fn make_commit_extra(&self, _number: u32, _parents: Vec<(&Sum, &CommitMeta)>) -> ExtraMeta {
            ExtraMeta::Text("text".to_string())
        }
    }
    
    let part_id = PartId::from_num(1);
    let csf = Classification::all();    // not used here: stored in header
    let mut state = PartState::<String>::new(part_id, csf.clone(), &mut MMNone {}).clone_mut();
    let data = "But I must explain to you how all this \
        mistaken idea of denouncing pleasure and praising pain was born and I \
        will give you a complete account of the system, and expound the \
        actual teachings of the great explorer of the truth, the master-\
        builder of human happiness. No one rejects, dislikes, or avoids \
        pleasure itself, because it is pleasure, but because those who do not \
        know how to pursue pleasure rationally encounter consequences that \
        are extremely painful. Nor again is there anyone who loves or pursues \
        or desires to obtain pain of itself, because it is pain, but because \
        occasionally circumstances occur in which toil and pain can procure \
        him some great pleasure. To take a trivial example, which of us ever \
        undertakes laborious physical exercise, except to obtain some \
        advantage from it? But who has any right to find fault with a man who \
        chooses to enjoy a pleasure that has no annoying consequences, or one \
        who avoids a pain that produces no resultant pleasure?";
    state.insert_new(data.to_string()).unwrap();
    let data = "arstneio[()]123%αρστνειο\
        qwfpluy-QWFPLUY—<{}>456+5≤≥φπλθυ−\
        zxcvm,./ZXCVM;:?`\"ç$0,./ζχψωμ~·÷";
    state.insert_new(data.to_string()).unwrap();
    
    struct MMTT {}
    impl MakeCommitMeta for MMTT {
        fn make_commit_extra(&self, _number: u32, _parents: Vec<(&Sum, &CommitMeta)>) -> ExtraMeta {
            ExtraMeta::Text("text".to_string())
        }
    }
    let state = PartState::from_mut(state, &mut MMTT {});
    
    let mut result = Vec::new();
    assert!(write_snapshot(&state, &mut result).is_ok());
    
    let state2 = read_snapshot(&mut &result[..], part_id, csf, HEAD_VERSIONS[HEAD_VERSIONS.len() - 1]).unwrap();
    assert_eq!(state, state2);
}
