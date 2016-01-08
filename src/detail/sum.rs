//! Pippin in-memory checksum operations

use std::io::{Write, Result};
use std::ops;
use std::fmt;


// #0018: when simd is stable, it could be used
// use simd::u8x16;
/// A convenient way to manage and manipulate a checksum.
/// 
/// This is not marked `Copy` but in any case should be fairly cheap to clone.
#[derive(Clone, PartialEq, Eq, Hash)]
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
        // #0018 : how to do a fixed-size array copy?
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
    
    /// Change self to self ^ other
    /// Note that this operation is its own inverse: x.permute(y).permute(y) == x.
    pub fn permute(&mut self, other: &Self) {
        (*self) = &*self ^ other;
    }
    
    /// Format as a string.
    /// 
    /// If `separate_pairs` is true, a space is inserted between every pair
    /// of chars (i.e. between every byte).
    pub fn as_string(&self, separate_pairs: bool) -> String {
        let step = if separate_pairs { 3 } else { 2 };
        let mut buf = vec![b' '; 32 * step];
        for i in 0..32 {
            let byte = self.s[i];
            buf[i*step] = HEX_CHARS[(byte / 16) as usize];
            buf[i*step + 1] = HEX_CHARS[(byte % 16) as usize];
        }
        String::from_utf8(buf).unwrap()
    }
    
    /// Return true if the given string is equivalent to the whole or an
    /// abreviated form of this checksum, rendered as hexadecimal using the
    /// symbols 0-9, A-F.
    /// 
    /// To improve matching, you may wish to strip spaces from and capitalise
    /// all letters of the string before calling this function.
    // #0019: I'm sure this function could be faster (in particular, by not using write!())
    pub fn matches_string(&self, string: &[u8]) -> bool {
        if string.len() > 2 * 32 {
            return false;
        }
        let mut buf = [0u8; 2];
        for i in 0..string.len() / 2 /*note: rounds down*/ {
            let byte = self.s[i];
            buf[0] = HEX_CHARS[(byte / 16) as usize];
            buf[1] = HEX_CHARS[(byte % 16) as usize];
            if string[i*2..i*2+2] != buf[..] {
                return false;
            }
        }
        if string.len() % 2 == 1 {
            buf[0] = HEX_CHARS[(self.s[string.len() / 2] / 16) as usize];
            if string[string.len() - 1] != buf[0] {
                return false;
            }
        }
        return true;
    }
}

const HEX_CHARS : &'static [u8; 16] = b"0123456789ABCDEF";

impl<'a> ops::BitXor for &'a Sum {
    type Output = Sum;
    fn bitxor(self, rhs: &'a Sum) -> Sum {
        // #0018: optimise XOR operation
        let mut result = Sum::zero();
        for i in 0..32 {
            result.s[i] = self.s[i] ^ rhs.s[i];
        }
        result
    }
}

impl fmt::Display for Sum {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}\
            {:02X}{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}\
            {:02X}{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}\
            {:02X}{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
            self.s[ 0], self.s[ 1], self.s[ 2], self.s[ 3], self.s[ 4], self.s[ 5], self.s[ 6], self.s[ 7],
            self.s[ 8], self.s[ 9], self.s[10], self.s[11], self.s[12], self.s[13], self.s[14], self.s[15],
            self.s[16], self.s[17], self.s[18], self.s[19], self.s[20], self.s[21], self.s[22], self.s[23],
            self.s[24], self.s[25], self.s[26], self.s[27], self.s[28], self.s[29], self.s[30], self.s[31])
    }
}
impl fmt::Debug for Sum {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} \
            {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} \
            {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} \
            {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X} {:02X}",
            self.s[ 0], self.s[ 1], self.s[ 2], self.s[ 3], self.s[ 4], self.s[ 5], self.s[ 6], self.s[ 7],
            self.s[ 8], self.s[ 9], self.s[10], self.s[11], self.s[12], self.s[13], self.s[14], self.s[15],
            self.s[16], self.s[17], self.s[18], self.s[19], self.s[20], self.s[21], self.s[22], self.s[23],
            self.s[24], self.s[25], self.s[26], self.s[27], self.s[28], self.s[29], self.s[30], self.s[31])
    }
}
