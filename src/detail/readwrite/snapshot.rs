//! Support for reading and writing Rust snapshots

use std::io::{Read, Write};
use chrono::UTC;
use crypto::digest::Digest;
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use super::{sum, fill};
use detail::{Sum, PartitionState};
use detail::{ElementT, Element};
use ::error::{Result, ReadError};

/// Read a snapshot of a set of elements from a stream.
/// 
/// This function reads to the end of the snapshot. It does not check whether
/// this is in fact the end of the file (or other data stream), though
/// according to the specified file format this should be the case.
pub fn read_snapshot<T: ElementT>(reader: &mut Read) ->
    Result<(PartitionState<T>, (u64, u64))>
{
    // A reader which calculates the checksum of what was read:
    let mut r = sum::HashReader::new256(reader);
    
    let mut pos: usize = 0;
    let mut buf = vec![0; 32];
    
    try!(fill(&mut r, &mut buf[0..32], pos));
    if buf[0..8] != *b"SNAPSHOT" {
        // note: we discard buf[8..16], the encoded date, for now
        return ReadError::err("unexpected contents (expected SNAPSHOT)", pos, (0, 8));
    }
    pos += 16;
    
    let (mut part_id0, mut part_id1) = (0, 0);
    if buf[16..22] == *b"PARTID" {
        part_id0 = read_u40(&buf[22..27]) << 24;
        part_id1 = read_u40(&buf[27..32]) << 24;
        try!(fill(&mut r, &mut buf[16..32], pos));
        pos += 8;
    } else {
        // #0017: warn about old file version
        // #0016: eventually don't support this version
        // Note that this *ought* to be checked with respect to the version
        // string in the header.
    }
    
    if buf[16..24] != *b"ELEMENTS" {
        return ReadError::err("unexpected contents (expected ELEMENTS)", pos, (16, 24));
    }
    let num_elts = try!((&buf[24..32]).read_u64::<BigEndian>()) as usize;    // #0015
    pos += 16;
    
    // #0016: here we set the "parent" sum to Sum::zero(). This isn't *correct*,
    // but since we won't be creating a commit from it it doesn't actually matter.
    let mut state = PartitionState::new(part_id0);
    for _ in 0..num_elts {
        try!(fill(&mut r, &mut buf[0..32], pos));
        if buf[0..8] != *b"ELEMENT\x00" {
            println!("buf: \"{}\", {:?}", String::from_utf8_lossy(&buf[0..8]), &buf[0..8]);
            return ReadError::err("unexpected contents (expected ELEMENT\\x00)", pos, (0, 8));
        }
        let ident = try!((&buf[8..16]).read_u64::<BigEndian>());
        pos += 16;
        
        if buf[16..24] != *b"BYTES\x00\x00\x00" {
            return ReadError::err("unexpected contents (expected BYTES\\x00\\x00\\x00)", pos, (16, 24));
        }
        let data_len = try!((&buf[24..32]).read_u64::<BigEndian>()) as usize;   // #0015
        pos += 16;
        
        let mut data = vec![0; data_len];
        try!(fill(&mut r, &mut data, pos));
        pos += data_len;
        
        let pad_len = 16 * ((data_len + 15) / 16) - data_len;
        if pad_len > 0 {
            try!(fill(&mut r, &mut buf[0..pad_len], pos));
            pos += pad_len;
        }
        
        let elt_sum = Sum::calculate(&data);
        try!(fill(&mut r, &mut buf[0..32], pos));
        if !elt_sum.eq(&buf[0..32]) {
            return ReadError::err("element checksum mismatch", pos, (0, 32));
        }
        pos += 32;
        
        let elt = try!(Element::from_vec(data));
        try!(state.insert_elt(ident, elt));
    }
    
    try!(fill(&mut r, &mut buf[0..16], pos));
    if buf[0..8] != *b"STATESUM" {
        return ReadError::err("unexpected contents (expected STATESUM)", pos, (0, 8));
    }
    pos += 8;
    if (try!((&buf[8..16]).read_u64::<BigEndian>()) as usize) != num_elts {
        return ReadError::err("unexpected contents (number of elements \
            differs from that previously stated)", pos, (8, 16));
    }
    pos += 8;
    
    try!(fill(&mut r, &mut buf[0..32], pos));
    if !state.statesum().eq(&buf[0..32]) {
        return ReadError::err("state checksum mismatch", pos, (0, 32));
    }
    pos += 32;
    
    assert_eq!( r.digest().output_bytes(), 32 );
    let mut sum32 = [0u8; 32];
    r.digest().result(&mut sum32);
    let mut r2 = r.into_inner();
    try!(fill(&mut r2, &mut buf[0..32], pos));
    if sum32 != buf[0..32] {
        return ReadError::err("checksum mismatch", pos, (0, 32));
    }
    
    Ok((state, (part_id0, part_id1)))
}

/// Write a snapshot of a set of elements to a stream
/// 
/// The snapshot is derived from a partition state, but also includes a
/// partition identifier range.
pub fn write_snapshot<T: ElementT>(state: &PartitionState<T>,
    part_range: (u64, u64), writer: &mut Write) -> Result<()>
{
    // A writer which calculates the checksum of what was written:
    let mut w = sum::HashWriter::new256(writer);
    
    let elts = state.map();
    
    // #0016: date shouldn't really be today but the time the snapshot was created
    try!(write!(&mut w, "SNAPSHOT{}", UTC::today().format("%Y%m%d")));
    
    let mut buf: [u8; 16] = *b"PARTID..........";
    write_u40(&mut buf[6..11], part_range.0 >> 24);
    write_u40(&mut buf[11..16], part_range.1 >> 24);
    try!(w.write(&buf));
    
    try!(w.write(b"ELEMENTS"));
    let num_elts = elts.len() as u64;  // #0015
    try!(w.write_u64::<BigEndian>(num_elts));
    
    let mut elt_buf = Vec::new();
    
    for (ident, elt) in elts {
        try!(w.write(b"ELEMENT\x00"));
        try!(w.write_u64::<BigEndian>(*ident));
        
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
    
    // We write the checksum we kept in memory, the idea being that in-memory
    // corruption will be detected on next load.
    try!(w.write(b"STATESUM"));
    try!(w.write_u64::<BigEndian>(num_elts));
    try!(state.statesum().write(&mut w));
    
    // Write the checksum of everything above:
    assert_eq!( w.digest().output_bytes(), 32 );
    let mut sum32 = [0u8; 32];
    w.digest().result(&mut sum32);
    let w2 = w.into_inner();
    try!(w2.write(&sum32));
    
    Ok(())
}

fn read_u40(buf: &[u8]) -> u64 {
    ((buf[0] as u64) << 32) +
    ((buf[1] as u64) << 24) +
    ((buf[2] as u64) << 16) +
    ((buf[3] as u64) << 08) +
    ((buf[4] as u64) << 00)
}
fn write_u40(buf: &mut[u8], val: u64) {
    buf[0] = ((val >> 32) & 0xFF) as u8;
    buf[1] = ((val >> 24) & 0xFF) as u8;
    buf[2] = ((val >> 16) & 0xFF) as u8;
    buf[3] = ((val >> 08) & 0xFF) as u8;
    buf[4] = ((val >> 00) & 0xFF) as u8;
}

#[test]
fn snapshot_writing() {
    let part_id = 1 << 24;
    let mut state = PartitionState::<String>::new(part_id);
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
    state.new_elt(Element::new(data.to_string())).unwrap();
    let data = "arstneio[()]123%αρστνειο\
        qwfpluy-QWFPLUY—<{}>456+5≤≥φπλθυ−\
        zxcvm,./ZXCVM;:?`\"ç$0,./ζχψωμ~·÷";
    state.new_elt(Element::new(data.to_string())).unwrap();
    
    let mut result = Vec::new();
    assert!(write_snapshot(&state, (part_id, 12 << 24), &mut result).is_ok());
    
    let (state2, part_range) = read_snapshot(&mut &result[..]).unwrap();
    assert_eq!(state, state2);
    assert_eq!(part_id, part_range.0);
}
