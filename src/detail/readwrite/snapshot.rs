/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Support for reading and writing Rust snapshots

use std::io::{Read, Write};
use std::rc::Rc;
use std::{u8, u32};

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use detail::readwrite::{sum};
use partition::{PartitionState, State};
use {ElementT, PartId, Sum, CommitMeta};
use detail::SUM_BYTES;
use error::{Result, ReadError};

/// Read a snapshot of a set of elements from a stream.
/// 
/// This function reads to the end of the snapshot. It does not check whether
/// this is in fact the end of the file (or other data stream), though
/// according to the specified file format this should be the case.
/// 
/// The `part_id` parameter is assigned to the `PartitionState` returned.
/// 
/// The file version affects how data is read. Get it from a header with
/// `header.ftype.ver()`.
pub fn read_snapshot<T: ElementT>(reader: &mut Read, part_id: PartId,
        file_ver: u32) -> Result<PartitionState<T>>
{
    // A reader which calculates the checksum of what was read:
    let mut r = sum::HashReader::new(reader);
    
    let mut pos: usize = 0;
    let mut buf = vec![0; 32];
    assert!(buf.len() >= SUM_BYTES);
    
    try!(r.read_exact(&mut buf[0..16]));
    let num_parents = if file_ver < 2016_02_27 {
        if buf[0..8] != *b"SNAPSHOT" {
            return ReadError::err("unexpected contents (expected SNAPSHOT)", pos, (0, 8));
        }
        0
    } else {
        if buf[0..6] != *b"SNAPSH" || buf[7] != b'U' {
            return ReadError::err("unexpected contents (expected SNAPSH_U where _ is any)", pos, (0, 8));
        }
        buf[6] as usize
    };
    let secs = try!((&buf[8..16]).read_i64::<BigEndian>());
    pos += 16;
    
    let meta = if file_ver >= 2016_02_22 {
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
        CommitMeta {
            number: 1,
            timestamp: 0,
            extra: None
        }
    };
    
    let mut parents = Vec::with_capacity(num_parents);
    for _ in 0..num_parents {
        try!(r.read_exact(&mut buf[0..SUM_BYTES]));
        parents.push(Sum::load(&buf[0..SUM_BYTES]));
    }
    
    try!(r.read_exact(&mut buf[0..16]));
    if buf[0..8] != *b"ELEMENTS" {
        return ReadError::err("unexpected contents (expected ELEMENTS)", pos, (0, 8));
    }
    let num_elts = try!((&buf[8..16]).read_u64::<BigEndian>()) as usize;    // #0015
    pos += 16;
    
    // #0016: here we don't set any parent sums. This isn't *correct*,
    // but since we won't be creating a commit from it it doesn't actually matter.
    let mut state = PartitionState::new_with(part_id, parents, meta);
    for _ in 0..num_elts {
        try!(r.read_exact(&mut buf[0..32]));
        if buf[0..8] != *b"ELEMENT\x00" {
            println!("buf: \"{}\", {:?}", String::from_utf8_lossy(&buf[0..8]), &buf[0..8]);
            return ReadError::err("unexpected contents (expected ELEMENT\\x00)", pos, (0, 8));
        }
        let ident = try!((&buf[8..16]).read_u64::<BigEndian>()).into();
        pos += 16;
        
        if buf[16..24] != *b"BYTES\x00\x00\x00" {
            return ReadError::err("unexpected contents (expected BYTES\\x00\\x00\\x00)", pos, (16, 24));
        }
        let data_len = try!((&buf[24..32]).read_u64::<BigEndian>()) as usize;   // #0015
        pos += 16;
        
        let mut data = vec![0; data_len];
        try!(r.read_exact(&mut data));
        pos += data_len;
        
        let pad_len = 16 * ((data_len + 15) / 16) - data_len;
        if pad_len > 0 {
            try!(r.read_exact(&mut buf[0..pad_len]));
            pos += pad_len;
        }
        
        let elt_sum = Sum::calculate(&data);
        try!(r.read_exact(&mut buf[0..SUM_BYTES]));
        if !elt_sum.eq(&buf[0..SUM_BYTES]) {
            return ReadError::err("element checksum mismatch", pos, (0, SUM_BYTES));
        }
        pos += SUM_BYTES;
        
        let elt = try!(T::from_vec(data));
        try!(state.insert_with_id(ident, Rc::new(elt)));
    }
    
    try!(r.read_exact(&mut buf[0..16]));
    if buf[0..8] == *b"ELTMOVES" /*versions from 20160201, optional*/ {
        let n_moves = try!((&buf[8..16]).read_u64::<BigEndian>()) as usize;    // #0015
        for _ in 0..n_moves {
            try!(r.read_exact(&mut buf[0..16]));
            let id0 = try!((&buf[0..8]).read_u64::<BigEndian>()).into();
            let id1 = try!((&buf[8..16]).read_u64::<BigEndian>()).into();
            state.set_move(id0, id1);
        }
        // re-fill buffer for next section:
        try!(r.read_exact(&mut buf[0..16]));
    }
    
    if buf[0..8] != *b"STATESUM" {
        return ReadError::err("unexpected contents (expected STATESUM or ELTMOVES)", pos, (0, 8));
    }
    pos += 8;
    if (try!((&buf[8..16]).read_u64::<BigEndian>()) as usize) != num_elts {
        return ReadError::err("unexpected contents (number of elements \
            differs from that previously stated)", pos, (8, 16));
    }
    pos += 8;
    
    try!(r.read_exact(&mut buf[0..SUM_BYTES]));
    if !state.statesum().eq(&buf[0..SUM_BYTES]) {
        return ReadError::err("state checksum mismatch", pos, (0, SUM_BYTES));
    }
    pos += SUM_BYTES;
    
    let sum = r.sum();
    let mut r = r.into_inner();
    try!(r.read_exact(&mut buf[0..SUM_BYTES]));
    if !sum.eq(&buf[0..SUM_BYTES]) {
        return ReadError::err("checksum invalid", pos, (0, SUM_BYTES));
    }
    
    trace!("Read snapshot (partition {} with {} elements): {}",
        part_id.into_num(), num_elts, state.statesum());
    Ok(state)
}

/// Write a snapshot of a set of elements to a stream
/// 
/// The snapshot is derived from a partition state, but also includes a
/// partition identifier range.
pub fn write_snapshot<T: ElementT>(state: &PartitionState<T>,
    writer: &mut Write) -> Result<()>
{
    trace!("Writing snapshot (partition {} with {} elements): {}",
        state.part_id().into_num(), state.num_avail(), state.statesum());
    
    // A writer which calculates the checksum of what was written:
    let mut w = sum::HashWriter::new(writer);
    
    let mut snapsh_u: [u8; 8] = *b"SNAPSH_U";
    assert!(state.parents().len() <= (u8::MAX as usize));
    snapsh_u[6] = state.parents().len() as u8;
    try!(w.write(&snapsh_u));
    try!(w.write_i64::<BigEndian>(state.meta().timestamp));
    
    try!(w.write(b"CNUM"));
    try!(w.write_u32::<BigEndian>(state.meta().number));
    
    if let Some(ref txt) = state.meta().extra {
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
    
    for parent in state.parents() {
        try!(parent.write(&mut w));
    }
    
    try!(w.write(b"ELEMENTS"));
    let num_elts = state.map().len() as u64;  // #0015
    try!(w.write_u64::<BigEndian>(num_elts));
    
    let mut elt_buf = Vec::new();
    
    for (ident, elt) in state.map() {
        try!(w.write(b"ELEMENT\x00"));
        try!(w.write_u64::<BigEndian>((*ident).into()));
        
        try!(w.write(b"BYTES\x00\x00\x00"));
        elt_buf.clear();
        try!(elt.write_buf(&mut &mut elt_buf));
        try!(w.write_u64::<BigEndian>(elt_buf.len() as u64 /* #0015 */));
        
        try!(w.write(&elt_buf));
        let pad_len = 16 * ((elt_buf.len() + 15) / 16) - elt_buf.len();
        if pad_len > 0 {
            let padding = [0u8; 15];
            try!(w.write(&padding[0..pad_len]));
        }
        
        let elt_sum = Sum::calculate(&elt_buf);
        try!(elt_sum.write(&mut w));
    }
    
    let moved = state.moved_map();
    if !moved.is_empty() {
        try!(w.write(b"ELTMOVES"));
        try!(w.write_u64::<BigEndian>(moved.len() as u64 /* #0015 */));
        for (ident, new_ident) in moved {
            try!(w.write_u64::<BigEndian>((*ident).into()));
            try!(w.write_u64::<BigEndian>((*new_ident).into()));
        }
    }
    
    // We write the checksum we kept in memory, the idea being that in-memory
    // corruption will be detected on next load.
    try!(w.write(b"STATESUM"));
    try!(w.write_u64::<BigEndian>(num_elts));
    try!(state.statesum().write(&mut w));
    
    // Write the checksum of everything above:
    let sum = w.sum();
    try!(sum.write(&mut w.into_inner()));
    
    Ok(())
}

#[test]
fn snapshot_writing() {
    let part_id = PartId::from_num(1);
    let v: Vec<u8> = (0u8..).take(SUM_BYTES).collect();
    let parent = Sum::load(&v);     // nonsense sum
    let meta = CommitMeta::new_from(5616, Some("text".to_string()));
    let mut state = PartitionState::<String>::new_with(part_id, vec![parent], meta);
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
    state.insert(data.to_string()).unwrap();
    let data = "arstneio[()]123%αρστνειο\
        qwfpluy-QWFPLUY—<{}>456+5≤≥φπλθυ−\
        zxcvm,./ZXCVM;:?`\"ç$0,./ζχψωμ~·÷";
    state.insert(data.to_string()).unwrap();
    
    let mut result = Vec::new();
    assert!(write_snapshot(&state, &mut result).is_ok());
    
    let state2 = read_snapshot(&mut &result[..], part_id, 2016_02_27).unwrap();
    assert_eq!(state, state2);
}
