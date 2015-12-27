//! Support for reading and writing Rust snapshots

use std::io::{Read, Write};
use chrono::UTC;
use crypto::digest::Digest;
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use super::{sum, fill};
use detail::{Sum, Element, PartitionState};
use ::error::{Error, Result};


/// Read a snapshot of a set of elements from a stream.
/// 
/// This function reads to the end of the snapshot. It does not check whether
/// this is in fact the end of the file (or other data stream), though
/// according to the specified file format this should be the case.
pub fn read_snapshot(reader: &mut Read) -> Result<PartitionState> {
    // A reader which calculates the checksum of what was read:
    let mut r = sum::HashReader::new256(reader);
    
    let mut pos: usize = 0;
    let mut buf = vec![0; 32];
    
    try!(fill(&mut r, &mut buf[0..32], pos));
    if buf[0..8] != *b"SNAPSHOT" {
        // note: we discard buf[8..16], the encoded date, for now
        return Err(Error::read("unexpected contents (expected SNAPSHOT)", pos, (0, 8)));
    }
    pos += 16;
    
    if buf[16..24] != *b"ELEMENTS" {
        return Err(Error::read("unexpected contents (expected ELEMENTS)", pos, (16, 24)));
    }
    let num_elts = try!((&buf[24..32]).read_u64::<BigEndian>()) as usize;    // #0015
    pos += 16;
    
    let mut state = PartitionState::new();
    for _ in 0..num_elts {
        try!(fill(&mut r, &mut buf[0..32], pos));
        if buf[0..8] != *b"ELEMENT\x00" {
            println!("buf: \"{}\", {:?}", String::from_utf8_lossy(&buf[0..8]), &buf[0..8]);
            return Err(Error::read("unexpected contents (expected ELEMENT\\x00)", pos, (0, 8)));
        }
        let ident = try!((&buf[8..16]).read_u64::<BigEndian>());
        pos += 16;
        
        if buf[16..24] != *b"BYTES\x00\x00\x00" {
            return Err(Error::read("unexpected contents (expected BYTES\\x00\\x00\\x00)", pos, (16, 24)));
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
            return Err(Error::read("element checksum mismatch", pos, (0, 32)));
        }
        pos += 32;
        
        try!(state.insert_elt(ident, Element::new(data, elt_sum)));
    }
    
    try!(fill(&mut r, &mut buf[0..16], pos));
    if buf[0..8] != *b"STATESUM" {
        return Err(Error::read("unexpected contents (expected STATESUM)", pos, (0, 8)));
    }
    pos += 8;
    if (try!((&buf[8..16]).read_u64::<BigEndian>()) as usize) != num_elts {
        return Err(Error::read("unexpected contents (number of elements \
            differs from that previously stated)", pos, (8, 16)));
    }
    pos += 8;
    
    try!(fill(&mut r, &mut buf[0..32], pos));
    if !state.statesum().eq(&buf[0..32]) {
        return Err(Error::read("state checksum mismatch", pos, (0, 32)));
    }
    pos += 32;
    
    assert_eq!( r.digest().output_bytes(), 32 );
    let mut sum32 = [0u8; 32];
    r.digest().result(&mut sum32);
    let mut r2 = r.into_inner();
    try!(fill(&mut r2, &mut buf[0..32], pos));
    if sum32 != buf[0..32] {
        return Err(Error::read("checksum mismatch", pos, (0, 32)));
    }
    
    Ok(state)
}

/// Write a snapshot of a set of elements to a stream
pub fn write_snapshot(state: &PartitionState, writer: &mut Write) -> Result<()>{
    // A writer which calculates the checksum of what was written:
    let mut w = sum::HashWriter::new256(writer);
    
    let elts = state.map();
    
    // #0016: date shouldn't really be today but the time the snapshot was created
    try!(write!(&mut w, "SNAPSHOT{}", UTC::today().format("%Y%m%d")));
    
    try!(w.write(b"ELEMENTS"));
    let num_elts = elts.len() as u64;  // #0015
    try!(w.write_u64::<BigEndian>(num_elts));
    
    for (ident, elt) in elts {
        try!(w.write(b"ELEMENT\x00"));
        try!(w.write_u64::<BigEndian>(*ident));
        
        try!(w.write(b"BYTES\x00\x00\x00"));
        try!(w.write_u64::<BigEndian>(elt.data_len() as u64 /* #0015 */));
        
        try!(w.write(&elt.data()));
        let pad_len = 16 * ((elt.data_len() + 15) / 16) - elt.data_len();
        if pad_len > 0 {
            let padding = [0u8; 15];
            try!(w.write(&padding[0..pad_len]));
        }
        
        // #0010: Now we store the checksum, should we use it here? Should we
        // #0010: rely on it or check and stop if it's wrong?
        let elt_sum = Sum::calculate(&elt.data());
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

#[test]
fn snapshot_writing() {
    let mut state = PartitionState::new();
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
    state.insert_elt(1, Element::from_str(data)).unwrap();
    let data = "arstneio[()]123%αρστνειο\
        qwfpluy-QWFPLUY—<{}>456+5≤≥φπλθυ−\
        zxcvm,./ZXCVM;:?`\"ç$0,./ζχψωμ~·÷";
    state.insert_elt(0xFEDCBA9876543210, Element::from_str(data)).unwrap();
    
    let mut result = Vec::new();
    assert!(write_snapshot(&state, &mut result).is_ok());
    
    let state2 = read_snapshot(&mut &result[..]).unwrap();
    assert_eq!(state, state2);
}
