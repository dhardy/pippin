/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Pippin utility functions

use std::cmp;

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
