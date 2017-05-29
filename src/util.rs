/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Pippin utility functions. These are not inherently related to Pippin, but are used by Pippin.

use std::cmp;
use std::fmt::{self, Write};

/// "trim" applied to generic arrays: while the last byte is pat, remove it.
///  
/// Performance is `O(l)` where `l = s.len()`.
pub fn rtrim<T: cmp::PartialEq>(s: &[T], pat: T) -> &[T] {
    let mut p = s.len();
    while p > 0 && s[p - 1] == pat { p -= 1; }
    &s[0..p]
}

#[test]
fn test_rtrim() {
    assert_eq!(rtrim(&[0, 15, 8], 15), &[0, 15, 8]);
    assert_eq!(rtrim(&[0, 15, 8, 8], 8), &[0, 15]);
    assert_eq!(rtrim(&[2.5], 2.5), &[]);
    assert_eq!(rtrim(&[], 'a'), &[] as &'static [char]);
}

/// Utility for displaying as a byte string
/// 
/// This is not optimised for performance.
pub struct ByteFormatter<'a> {
    bytes: &'a [u8]
}
impl<'a> ByteFormatter<'a> {
    /// Construct, from a byte slice
    pub fn from(bytes: &'a[u8]) -> ByteFormatter<'a> {
        ByteFormatter { bytes: bytes }
    }
}
impl<'a> fmt::Display for ByteFormatter<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for b in self.bytes {
            if *b == b'\\' {
                write!(f, "\\\\")?;
            } else if *b == b'"' {
                write!(f, "\\\"")?;
            } else if *b == b'\'' {
                write!(f, "\\\'")?;
            } else if *b >= b' ' && *b <= b'~' {
                f.write_char(*b as char)?;
            } else {
                write!(f, "\\x{:02x}", b)?;
            }
        }
        Ok(())
    }
}

/// Utility struct to write a byte array in hex.
pub struct HexFormatter<'a> {
    bytes: &'a [u8],
}
impl<'a> HexFormatter<'a> {
    /// Construct, passing bytes to display as a line. Line length is
    /// determined by the length of this slice.
    pub fn line(bytes: &'a [u8]) -> HexFormatter<'a> {
        HexFormatter { bytes: bytes }
    }
}
impl <'a> fmt::Display for HexFormatter<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        const HEX: &'static str = "0123456789ABCDEF";
        let line = self.bytes;
        for i in 0..line.len() {
            let (high,low) = (line[i] as usize / 16, line[i] as usize & 0xF);
            write!(f, "{}{} ", &HEX[high..(high+1)], &HEX[low..(low+1)])?;
        }
        let mut v: Vec<u8> = Vec::from(line);
        for i in 0..v.len() {
            let c = v[i];
            // replace spaces, tabs and undisplayable characters:
            if c <= 0x32 || c == 0x7F { v[i] = b'.'; }
        }
        writeln!(f, "{}", String::from_utf8_lossy(&v))?;
        Ok(())
    }
}
