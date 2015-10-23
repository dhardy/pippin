//! Pippin change/commit log reading and writing

//! Support for reading and writing Rust snapshots

use std::io::{Read, Write};
use std::collections::HashMap;
use crypto::digest::Digest;
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use ::{Element};
use super::{sum, fill};
use detail::{Sum, Commit};
use detail::commits::EltChange;
use ::error::{Error, Result};

/// Implement this to use read_log().
pub trait CommitReceiver {
    /// Implement to receive a commit once it has been read. Return true to
    /// continue reading or false to stop reading more commits.
    fn receive(&mut self, commit: Commit) -> bool;
}

/// Read a commit log from a stream
pub fn read_log(reader_: &mut Read, receiver: &mut CommitReceiver) -> Result<()> {
    let mut reader = reader_;
    let mut pos: usize = 0;
    let mut buf = vec![0; 32];
    
    try!(fill(&mut reader, &mut buf[0..16], pos));
    if buf[0..16] != *b"COMMIT LOG\x00\x00\x00\x00\x00\x00" {
        return Err(Error::read("unexpected contents (expected \
            COMMIT LOG\\x00\\x00\\x00\\x00\\x00\\x00)", pos, (0, 16)));
    }
    pos += 16;
    
    // We now read commits. Since new commits can simply be appended to the
    // file, we only know we're at the end if we hit EOF. This is the only
    // condition where encountering EOF is not an error.
    loop {
        // A reader which calculates the checksum of what was read:
        let mut r = sum::HashReader::new256(reader);
        
        let l = try!(r.read(&mut buf[0..16]));
        if l == 0 { break; /*end of file (EOF)*/ }
        if l < 16 { try!(fill(&mut r, &mut buf[l..16], pos)); /*not EOF, buf haven't filled buffer*/ }
        
        if buf[0..8] != *b"COMMIT\x00\x00" {
            return Err(Error::read("unexpected contents (expected COMMIT\\x00\\x00)", pos, (0, 8)));
        }
        // TODO: timestamp
        pos += 16;
        
        try!(fill(&mut r, &mut buf[0..32], pos));
        let parent_sum = Sum::load(&buf);
        pos += 32;
        
        try!(fill(&mut r, &mut buf[0..16], pos));
        if buf[0..8] != *b"ELEMENTS" {
            return Err(Error::read("unexpected contents (expected ELEMENTS)", pos, (0, 8)));
        }
        let num_elts = try!((&buf[8..16]).read_u64::<BigEndian>()) as usize;   //TODO is cast safe?
        pos += 16;
        
        let mut changes = HashMap::new();
        
        for _ in 0..num_elts {
            try!(fill(&mut r, &mut buf[0..16], pos));
            if buf[0..4] != *b"ELT " {
                return Err(Error::read("unexpected contents (expected ELT\\x20)", pos, (0, 4)));
            }
            let elt_id = try!((&buf[8..16]).read_u64::<BigEndian>());
            let change_t = match &buf[4..8] {
                b"DEL\x00" => { Change::Delete },
                b"INS\x00" => { Change::Insert },
                b"REPL" => { Change::Replace },
                _ => {
                    return Err(Error::read("unexpected contents (expected one \
                        of DEL\\x00, INS\\x00, REPL)", pos, (4, 8)));
                }
            };
            pos += 16;
            
            let change = match change_t {
                Change::Delete => EltChange::deletion(),
                Change::Insert | Change::Replace => {
                    try!(fill(&mut r, &mut buf[0..16], pos));
                    if buf[0..8] != *b"ELT DATA" {
                        return Err(Error::read("unexpected contents (expected ELT DATA)", pos, (0, 8)));
                    }
                    let data_len = try!((&buf[8..16]).read_u64::<BigEndian>()) as usize;   //TODO is cast safe?
                    pos += 16;
                    
                    let mut data = vec![0; data_len];
                    try!(fill(&mut r, &mut data, pos));
                    pos += data_len;
                    
                    let pad_len = 16 * ((data_len + 15) / 16) - data_len;
                    if pad_len > 0 {
                        try!(fill(&mut r, &mut buf[0..pad_len], pos));
                        pos += pad_len;
                    }
                    
                    let data_sum = Sum::calculate(&data);
                    try!(fill(&mut r, &mut buf[0..32], pos));
                    if !data_sum.eq(&buf[0..32]) {
                        return Err(Error::read("element checksum mismatch", pos, (0, 32)));
                    }
                    pos += 32;
                    
                    let elt = Element::new(data, data_sum);
                    match change_t {
                        Change::Insert => EltChange::insertion(elt),
                        Change::Replace => EltChange::replacement(elt),
                        _ => panic!()
                    }
                }
            };
            changes.insert(elt_id, change);
        }
        
        try!(fill(&mut r, &mut buf[0..32], pos));
        let commit_sum = Sum::load(&buf);
        pos += 32;
        
        assert_eq!( r.digest().output_bytes(), 32 );
        let mut sum32 = [0u8; 32];
        r.digest().result(&mut sum32);
        reader = r.into_inner();
        try!(fill(&mut reader, &mut buf[0..32], pos));
        if sum32 != buf[0..32] {
            return Err(Error::read("checksum mismatch", pos, (0, 32)));
        }
        
        // TODO: now we've read a commit...
        let cont = receiver.receive(Commit::new(commit_sum, parent_sum, changes));
        if !cont { break; }
    }
    
    enum Change {
        Delete, Insert, Replace
    }
    
    Ok(())
}

/// Write the section identifier at the start of a commit log
//TODO: do we actually need this?
pub fn start_log(writer: &mut Write) -> Result<()> {
    try!(writer.write(b"COMMIT LOG\x00\x00\x00\x00\x00\x00"));
    Ok(())
}

/// Write a single commit to a stream
pub fn write_commit(commit: &Commit, writer: &mut Write) -> Result<()> {
    // A writer which calculates the checksum of what was written:
    let mut w = sum::HashWriter::new256(writer);
    
    //TODO: replace dots with timestamp or whatever...
    try!(w.write(b"COMMIT\x00\x00........"));
    
    // Parent statesum:
    try!(commit.parent().write(&mut w));
    
    try!(w.write(b"ELEMENTS"));
    try!(w.write_u64::<BigEndian>(commit.num_changes() as u64));       // TODO: is cast safe?
    
    for (elt_id,change) in commit.changes_iter() {
        let marker = match change {
            &EltChange::Deletion => b"ELT DEL\x00",
            &EltChange::Insertion(_) => b"ELT INS\x00",
            &EltChange::Replacement(_) => b"ELT REPL",
        };
        try!(w.write(marker));
        try!(w.write_u64::<BigEndian>(*elt_id));
        if let Some(elt) = change.element() {
            try!(w.write(b"ELT DATA"));
            try!(w.write_u64::<BigEndian>(elt.data_len() as u64));      // TODO: is cast safe?
            
            try!(w.write(&elt.data()));
            let pad_len = 16 * ((elt.data_len() + 15) / 16) - elt.data_len();
            if pad_len > 0 {
                let padding = [0u8; 15];
                try!(w.write(&padding[0..pad_len]));
            }
            
            try!(elt.sum().write(&mut w));
        }
    }
    
    try!(commit.statesum().write(&mut w));
    
    assert_eq!( w.digest().output_bytes(), 32 );
    let mut sum32 = [0u8; 32];
    w.digest().result(&mut sum32);
    let w2 = w.into_inner();
    try!(w2.write(&sum32));
    
    Ok(())
}

#[test]
fn commit_write_read(){
    // Note that we can make up completely nonsense commits here. Element
    // checksums must still match but state sums don't need to since we won't
    // be reproducing states. So lets make some fun sums!
    let mut v: Vec<u8> = (0u8..).take(32).collect();
    let seq = Sum::load(&v);
    v = (0u8..).map(|x| x.wrapping_mul(x)).take(32).collect();
    let squares = Sum::load(&v);
    v = (1u8..).map(|x| x.wrapping_add(7u8).wrapping_mul(3u8)).take(32).collect();
    let nonsense = Sum::load(&v);
    v = (1u8..).map(|x| x.wrapping_mul(x).wrapping_add(5u8.wrapping_mul(x)).wrapping_add(11u8)).take(32).collect();
    let quadr = Sum::load(&v);
    
    let mut changes = HashMap::new();
    changes.insert(3, EltChange::insertion(Element::from_str("three")));
    changes.insert(4, EltChange::insertion(Element::from_str("four")));
    changes.insert(5, EltChange::insertion(Element::from_str("five")));
    let commit_1 = Commit::new(seq, squares, changes);
    
    changes = HashMap::new();
    changes.insert(1, EltChange::deletion());
    changes.insert(9, EltChange::replacement(Element::from_str("NINE!")));
    changes.insert(5, EltChange::insertion(Element::from_str("five again?")));
    let commit_2 = Commit::new(nonsense, quadr, changes);
    
    let mut obj = Vec::new();
    assert!(start_log(&mut obj).is_ok());
    assert!(write_commit(&commit_1, &mut obj).is_ok());
    assert!(write_commit(&commit_2, &mut obj).is_ok());
    
    impl CommitReceiver for Vec<Commit> {
        fn receive(&mut self, commit: Commit) -> bool { self.push(commit); true }
    }
    let mut commits = Vec::new();
    match read_log(&mut &obj[..], &mut commits) {
        Ok(()) => {},
        Err(Error::Read(e)) => {
            panic!("read_log failed: {}", e.display(&obj));
        },
        Err(e) => {
            panic!("read_log failed: {}", e);
        }
    }
    
    assert_eq!(commits.len(), 2);
    assert_eq!(commits[0], commit_1);
    assert_eq!(commits[1], commit_2);
}
