//! Support for reading and writing Rust snapshots

use std::{io, fmt, ops};
use std::io::Write;
use std::collections::HashMap;
use chrono::UTC;
use crypto::sha2::Sha256;
use crypto::digest::Digest;
use byteorder::{BigEndian, WriteBytesExt};

use ::{Repo, Element};
use ::detail::sum;
use ::error::{Result};

// NOTE: when simd is stable, it could be used
// use simd::u8x16;
/// Possibly a more efficient way to represent a checksum
struct Sum {
//     s1: u8x16, s2: u8x16
    s: [u8; 32]
}

impl Sum {
    /// A "sum" containing all zeros
    fn zero() -> Sum {
//         Sum { s1: u8x16::splat(0), s2: u8x16::splat(0) }
        Sum { s: [0u8; 32] }
    }
    
    /// Load from a u8 array
    fn load(arr: &[u8]) -> Sum {
        assert_eq!(arr.len(), 32);
//         Sum { s1: u8x16::load(&arr, 0), s2: u8x16::load(&arr, 16) }
        //TODO there must be a better way than this!
        let mut result = Sum::zero();
        for i in 0..32 {
            result.s[i] = arr[i];
        }
        result
    }
    
    /// Calculate from some data
    fn calculate(data: &[u8]) -> Sum {
        let mut hasher = Sha256::new();
        hasher.input(&data);
        let mut buf = [0u8; 32];
        assert_eq!(hasher.output_bytes(), buf.len());
        hasher.result(&mut buf);
        Sum::load(&buf)
    }
    
    /// Write the checksum bytes to a stream
    fn write(&self, w: &mut Write) -> Result<()> {
//         let mut buf = [0u8; 32];
//         s1.store(&mut buf, 0);
//         s2.store(&mut buf, 16);
        try!(w.write(&self.s));
        Ok(())
    }
}

impl ops::BitXor for Sum {
    type Output = Self;
    fn bitxor(self, rhs: Sum) -> Sum {
        //TODO optimise
        let mut result = Sum::zero();
        for i in 0..32 {
            result.s[i] = self.s[i] ^ rhs.s[i];
        }
        result
    }
}

/// Write a snapshot of a set of elements to a stream
fn write_snapshot(elts: &Repo, writer: &mut Write) -> Result<()>{
    // A writer which calculates the checksum of what was written:
    let mut w = sum::HashWriter::new256(writer);
    
    //TODO: date shouldn't really be today but the time the snapshot was created
    try!(write!(&mut w, "SNAPSHOT{}", UTC::today().format("%Y%m%d")));
    
    // TODO: state/commit identifier stuff
    
    try!(w.write(b"ELEMENTS"));
    let num_elts = elts.elements.len() as u64;  // TODO: can we assume cast is safe?
    try!(w.write_u64::<BigEndian>(num_elts));
    
    // Note: for now we calculate the state checksum whenever we need it. It
    // may make more sense to store it and/or element sums in the future.
    let mut state_sum = Sum::zero();
    for (ident, elt) in &elts.elements {
        try!(w.write(b"ELEMENT\xbb"));
        try!(w.write_u64::<BigEndian>(*ident));
        
        try!(w.write(&elt.data));
        
        let elt_sum = Sum::calculate(&elt.data);
        try!(elt_sum.write(&mut w));
        
        state_sum = state_sum ^ elt_sum;
    }
    
    try!(w.write(b"STATESUM"));
    try!(w.write_u64::<BigEndian>(num_elts));
    try!(state_sum.write(&mut w));
    
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
    let mut elts = HashMap::new();
    elts.insert(1, Element { data: "But I must explain to you how all this \
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
        who avoids a pain that produces no resultant pleasure?"
        .as_bytes().to_vec() } );
    elts.insert(0xFEDCBA9876543210, Element { data: "arstneio[()]123%αρστνειο\
        qwfpluy-QWFPLUY—<{}>456+5≤≥φπλθυ−\
        zxcvm,./ZXCVM;:?`\"ç$0,./ζχψωμ~·÷".as_bytes().to_vec() });
    let repo = Repo {
        name: "My repo".to_string(),
        elements: elts
    };
    
    let mut result = Vec::new();
    write_snapshot(&repo, &mut result);
    // TODO: actually verify the output. Maybe just read it back in and compare
    // the two `Repo`s.
    assert_eq!(result.len(), 1296);
}
