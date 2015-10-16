//! Pippin in-memory checksum operations

use std::io::{Write, Result};
use std::ops;
use std::fmt;


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

impl fmt::Debug for Sum {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} \
            {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} \
            {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} \
            {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X}",
            self.s[0], self.s[1], self.s[2], self.s[3], self.s[4], self.s[5], self.s[6], self.s[7],
            self.s[8], self.s[9], self.s[10], self.s[11], self.s[12], self.s[13], self.s[14], self.s[15],
            self.s[16], self.s[17], self.s[18], self.s[19], self.s[20], self.s[21], self.s[22], self.s[23],
            self.s[24], self.s[25], self.s[26], self.s[27], self.s[28], self.s[29], self.s[30], self.s[31])
    }
}
