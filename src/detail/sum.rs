//! Pippin in-memory checksum operations

use std::io::{Write, Result};
use std::ops;


// NOTE: when simd is stable, it could be used
// use simd::u8x16;
/// A convenient way to manage and manipulate a checksum
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct Sum {
//     s1: u8x16, s2: u8x16
    s: [u8; 32]
}

impl Sum {
    /// A "sum" containing all zeros
    pub fn zero() -> Sum {
//         Sum { s1: u8x16::splat(0), s2: u8x16::splat(0) }
        Sum { s: [0u8; 32] }
    }
    
    /// True if sum equals that in a buffer
    pub fn eq(&self, arr: &[u8]) -> bool {
        assert_eq!(arr.len(), 32);
        for i in 0..32 {
            if self.s[i] != arr[i] { return false; }
        }
        return true;
    }
    
    /// Load from a u8 array
    pub fn load(arr: &[u8]) -> Sum {
        assert_eq!(arr.len(), 32);
//         Sum { s1: u8x16::load(&arr, 0), s2: u8x16::load(&arr, 16) }
        //TODO there must be a better way than this!
        let mut result = Sum::zero();
        for i in 0..32 {
            result.s[i] = arr[i];
        }
        result
    }
    
    /// Write the checksum bytes to a stream
    pub fn write(&self, w: &mut Write) -> Result<()> {
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
