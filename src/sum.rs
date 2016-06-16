/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Pippin in-memory checksum operations

use std::io::{Write, Result};
use std::ops;
use std::fmt;

use ::util::ByteFormatter;


/// Number of bytes in a Sum.
// #0018: it might be possible to move this inside Sum in future versions of Rust
pub const BYTES: usize = 32;
const BYTES_U8: u8 = BYTES as u8;


// #0031: when simd is stable, it could be used
// use simd::u8x16;
/// A convenient way to manage and manipulate a checksum.
/// 
/// This is not marked `Copy` but in any case should be fairly cheap to clone.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Sum {
//     s1: u8x16, s2: u8x16
    s: [u8; BYTES]
}

impl Sum {
    /// A "sum" containing all zeros
    pub fn zero() -> Sum {
//         Sum { s1: u8x16::splat(0), s2: u8x16::splat(0) }
        Sum { s: [0u8; BYTES] }
    }
    
    /// True if sum equals that in a buffer
    pub fn eq(&self, arr: &[u8]) -> bool {
        assert_eq!(arr.len(), BYTES);
        for i in 0..BYTES {
            if self.s[i] != arr[i] { return false; }
        }
        return true;
    }
    
    /// Load from a u8 array
    pub fn load(arr: &[u8]) -> Sum {
        assert_eq!(arr.len(), BYTES);
//         Sum { s1: u8x16::load(&arr, 0), s2: u8x16::load(&arr, 16) }
        let mut s = [0u8; BYTES];
        s.clone_from_slice(arr);
        Sum{ s: s }
    }
    
    /// Write the checksum bytes to a stream
    pub fn write(&self, w: &mut Write) -> Result<()> {
//         let mut buf = [0u8; 32];
//         s1.store(&mut buf, 0);
//         s2.store(&mut buf, 16);
        try!(w.write_all(&self.s));
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
        let mut buf = vec![b' '; BYTES * step];
        for i in 0..BYTES {
            let byte = self.s[i];
            buf[i*step] = HEX_CHARS[(byte / BYTES_U8) as usize];
            buf[i*step + 1] = HEX_CHARS[(byte % BYTES_U8) as usize];
        }
        String::from_utf8(buf).unwrap()
    }
    
    /// Allow formatting as a byte string
    pub fn byte_string(&self) -> ByteFormatter {
        ByteFormatter::from(&self.s)
    }
    
    /// Return true if the given string is equivalent to the whole or an
    /// abreviated form of this checksum, rendered as hexadecimal using the
    /// symbols 0-9, A-F.
    /// 
    /// To improve matching, you may wish to strip spaces from and capitalise
    /// all letters of the string before calling this function.
    // #0019: I'm sure this function could be faster (in particular, by not using write!())
    pub fn matches_string(&self, string: &[u8]) -> bool {
        if string.len() > 2 * BYTES {
            return false;
        }
        let mut buf = [0u8; 2];
        for i in 0..string.len() / 2 /*note: rounds down*/ {
            let byte: u8 = self.s[i];
            buf[0] = HEX_CHARS[(byte >> 4) as usize];
            buf[1] = HEX_CHARS[(byte & 0xF) as usize];
            if string[i*2..i*2+2] != buf[..] {
                return false;
            }
        }
        if string.len() % 2 == 1 {
            buf[0] = HEX_CHARS[(self.s[string.len() / 2] / BYTES_U8) as usize];
            if string[string.len() - 1] != buf[0] {
                return false;
            }
        }
        return true;
    }
    
    /// Write a formatted version to a formatter
    fn fmt_to(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // #0019: this could probably be faster
        write!(f, "{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}\
            {:02X}{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
            self.s[ 0], self.s[ 1], self.s[ 2], self.s[ 3], self.s[ 4], self.s[ 5], self.s[ 6], self.s[ 7],
            self.s[ 8], self.s[ 9], self.s[10], self.s[11], self.s[12], self.s[13], self.s[14], self.s[15])
    }
}

const HEX_CHARS : &'static [u8; 16] = b"0123456789ABCDEF";

impl<'a> ops::BitXor for &'a Sum {
    type Output = Sum;
    fn bitxor(self, rhs: &'a Sum) -> Sum {
        // #0031: optimise XOR operation
        let mut result = Sum::zero();
        for i in 0..BYTES {
            result.s[i] = self.s[i] ^ rhs.s[i];
        }
        result
    }
}

impl fmt::Display for Sum {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.fmt_to(f)
    }
}
impl fmt::Debug for Sum {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.fmt_to(f)
    }
}
