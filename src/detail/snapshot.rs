//! Support for reading and writing Rust snapshots

use std::{io, fmt, ops};
use std::io::Write;
use chrono::UTC;
use crypto::sha2::Sha256;
use crypto::digest::Digest;

use ::Repo;
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
    
    try!(write!(&mut w, "SNAPSHOT{}", UTC::today().format("%Y%m%d")));
    
    // Note: for now we calculate the state checksum whenever we need it. It
    // may make more sense to store it and/or element sums in the future.
    let mut state_sum = Sum::zero();
    for elt in elts.elements.values() {
        state_sum = state_sum ^ Sum::calculate(&elt.data);
    }
    try!(state_sum.write(&mut w));
    
    // TODO: per-element data
    // TODO: number of elements
    // TODO: time stamp
    // TODO: commit number?
    // TODO: checksum of data written
    
    Ok(())
}
