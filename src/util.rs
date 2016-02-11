/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Pippin utility functions

use std::cmp;
use std::str::from_utf8;
use std::fmt;

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
                try!(write!(f, "\\\\"));
            } else if *b >= b' ' && *b <= b'~' {
                // TODO: this is a horrible way to write a char!
                let v = vec![*b];
                try!(write!(f, "{}", from_utf8(&v).unwrap()));
            } else {
                try!(write!(f, "\\x{:02x}", b));
            }
        }
        Ok(())
    }
}
