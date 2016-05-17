/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Internal error structs used by Pippin

use std::{io, fmt, result};
use std::path::PathBuf;
use std::cmp::{min, max};

/// Our custom result type
pub type Result<T, E = Error> = result::Result<T, E>;

/// Our custom compound error type
pub type Error = Box<ErrorTrait>;

pub use std::error::Error as ErrorTrait;


// —————  ReadError  —————
/// For read errors; adds a read position
#[derive(PartialEq, Debug)]
pub struct ReadError {
    msg: &'static str,
    pos: usize,
    off_start: usize,
    off_end: usize,
}
impl ReadError {
    /// Create a "read" error with read position
    pub fn new(msg: &'static str, pos: usize, offset: (usize, usize)) -> ReadError {
        let (o0, o1) = offset;
        ReadError { msg: msg, pos: pos, off_start: o0, off_end: o1 }
    }
    /// New instance, wrapped with `Err`
    pub fn err<T>(msg: &'static str, pos: usize, offset: (usize, usize)) -> Result<T> {
        Err(box ReadError::new(msg, pos, offset))
    }
    /// Return an object which can be used in format expressions.
    /// 
    /// Usage: `println!("{}", err.display(&buf));`
    pub fn display<'a>(&'a self, data: &'a [u8]) -> ReadErrorFormatter<'a> {
        ReadErrorFormatter { err: self, data: data }
    }
}
impl ErrorTrait for ReadError {
    fn description(&self) -> &str { self.msg }
}
impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter) -> result::Result<(), fmt::Error> {
        write!(f, "read error at position {}, offset ({}, {}): {}", 
                self.pos, self.off_start, self.off_end, self.msg)
    }
}
/// Type used to format an error message
pub struct ReadErrorFormatter<'a> {
    err: &'a ReadError,
    data: &'a [u8],
}
impl<'a> fmt::Display for ReadErrorFormatter<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> result::Result<(), fmt::Error> {
        const SPACE: &'static str = "                        ";
        const MARK: &'static str = "^^^^^^^^^^^^^^^^^^^^^^^^";
        
        try!(writeln!(f, "read error (pos {}, offset ({}, {})): {}", self.err.pos,
            self.err.off_start, self.err.off_end, self.err.msg));
        let start = self.err.pos + 8 * (self.err.off_start / 8);
        let end = self.err.pos + 8 * ((self.err.off_end + 7) / 8);
        for line_start in (start..end).step_by(8) {
            if line_start + 8 > self.data.len() {
                try!(writeln!(f, "insufficient data to display!"));
                break;
            }
            try!(write_hex_line(&self.data[line_start..line_start+8], f));
            let p0 = max(self.err.pos + self.err.off_start, line_start) - line_start;
            let p1 = min(self.err.pos + self.err.off_end, line_start + 8) - line_start;
            assert!(p0 <= p1 && p1 <= 8);
            try!(write!(f, "{}{}{}", &SPACE[0..(3*p0)], &MARK[(3*p0)..(3*p1-1)], &SPACE[(3*p1-1)..24]));
            try!(writeln!(f, "{}{}", &SPACE[0..p0], &MARK[p0..p1]));
        }
        Ok(())
    }
}
// Utility function: dump a line as hex
// 
// Line length is determined by the slice passed.
fn write_hex_line(line: &[u8], f: &mut fmt::Formatter) -> result::Result<(), fmt::Error> {
    const HEX: &'static str = "0123456789ABCDEF";
    
    for i in 0..line.len() {
        let (high,low) = (line[i] as usize / 16, line[i] as usize & 0xF);
        try!(write!(f, "{}{} ", &HEX[high..(high+1)], &HEX[low..(low+1)]));
    }
    let mut v: Vec<u8> = Vec::from(line);
    for i in 0..v.len() {
        let c = v[i];
        // replace spaces, tabs and undisplayable characters:
        if c <= 0x32 || c == 0x7F { v[i] = b'.'; }
    }
    try!(writeln!(f, "{}", String::from_utf8_lossy(&v)));
    Ok(())
}


// —————  ArgError  ————
/// Any error where an invalid argument was supplied
#[derive(PartialEq, Debug)]
pub struct ArgError {
    msg: &'static str
}
impl ArgError {
    /// Create an "invalid argument" error
    pub fn new(msg: &'static str) -> ArgError {
        ArgError{ msg: msg }
    }
    /// New instance, wrapped with `Err`
    pub fn err<T>(msg: &'static str) -> Result<T> {
        Err(box ArgError::new(msg))
    }
}
impl ErrorTrait for ArgError {
    fn description(&self) -> &str { self.msg }
}
impl fmt::Display for ArgError {
    fn fmt(&self, f: &mut fmt::Formatter) -> result::Result<(), fmt::Error> {
        write!(f, "invalid argument: {}", self.msg)
    }
}


// —————  ElementOp  —————
/// Reason for an element retrieval/insertion/deletion/etc. operation failing.
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum ElementOp {
    /// The element was simply not found
    NotFound,
    /// Unable to find a free element identifier for a new element
    IdGenFailure,
    /// Wrong partition identifier. This should only be returned for operations
    /// on a specified partition, and indicates that the given identifier is
    /// from another partition.
    WrongPartition,
    /// Identifier already in use. An insertion failed since the given
    /// identifier is already in use.
    IdClash,
    /// Unable to proceed because classification failed
    ClassifyFailure,
    /// The relevant partition is not loaded within a repository
    NotLoaded,
}
impl ErrorTrait for ElementOp {
    fn description(&self) -> &'static str {
        match *self {
            ElementOp::NotFound => "element not found",
            ElementOp::IdGenFailure => "id generation failed to find a free identifier",
            ElementOp::WrongPartition => "operation on a partition uses element identifier from another partition",
            ElementOp::IdClash => "identifier already in use",
            ElementOp::ClassifyFailure => "classification of element failed",
            ElementOp::NotLoaded => "partition must be loaded",
        }
    }
}
impl fmt::Display for ElementOp {
    fn fmt(&self, f: &mut fmt::Formatter) -> result::Result<(), fmt::Error> {
        write!(f, "{}", self.description())
    }
}


// —————  PatchOp  —————
/// Reason for a `push_commit` / `push_state` / commit patch operation failing.
/// 
/// Any ElementOp can automatically be converted to PatchOp::PatchApply.
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum PatchOp {
    /// Parent state not found
    NoParent,
    /// Incorrect parent supplied to patch operationn
    WrongParent,
    /// Patch fails to apply cleanly
    PatchApply,
}
impl ErrorTrait for PatchOp {
    fn description(&self) -> &'static str {
        match *self {
            PatchOp::NoParent => "parent state of commit not found",
            PatchOp::WrongParent => "applying commit patch failed: wrong parent",
            PatchOp::PatchApply => "applying commit patch failed: data mismatch",
        }
    }
}
impl fmt::Display for PatchOp {
    fn fmt(&self, f: &mut fmt::Formatter) -> result::Result<(), fmt::Error> {
        write!(f, "{}", self.description())
    }
}
impl From<ElementOp> for PatchOp {
    fn from(e: ElementOp) -> PatchOp {
        // Possibly WrongPartition, ClassifyFailure and NotLoaded shouldn't map
        // like this.
        trace!("casting ElementOp '{}' to PatchOp::PatchApply", e.description());
        PatchOp::PatchApply
    }
}


// —————  ReplayError  —————
/// Errors in log replay (due either to corruption or providing incompatible
/// states and commit logs)
#[derive(PartialEq, Debug)]
pub struct ReplayError {
    msg: &'static str
}
impl ReplayError {
    /// Create a "log replay" error
    pub fn new(msg: &'static str) -> ReplayError {
        ReplayError { msg: msg }
    }
    /// New instance, wrapped with `Err`
    pub fn err<T>(msg: &'static str) -> Result<T> {
        Err(box ReplayError::new(msg))
    }
}
impl ErrorTrait for ReplayError {
    fn description(&self) -> &str { self.msg }
}
impl fmt::Display for ReplayError {
    fn fmt(&self, f: &mut fmt::Formatter) -> result::Result<(), fmt::Error> {
        write!(f, "failed to recreate state from log: {}", self.msg)
    }
}


// —————  PathError  —————
/// Error messages about some path on the file system
#[derive(PartialEq, Debug)]
pub struct PathError {
    msg: &'static str,
    path: PathBuf,
}
impl PathError {
    /// Create a "path" error. Will be displayed as
    /// `println!("Error: {}: {}", msg, path.display());`.
    pub fn new(msg: &'static str, path: PathBuf) -> PathError {
        PathError { msg: msg, path: path }
    }
    /// New instance, wrapped with `Err`
    pub fn err<T>(msg: &'static str, path: PathBuf) -> Result<T> {
        Err(box PathError::new(msg, path))
    }
}
impl ErrorTrait for PathError {
    fn description(&self) -> &str { self.msg }
}
impl fmt::Display for PathError {
    fn fmt(&self, f: &mut fmt::Formatter) -> result::Result<(), fmt::Error> {
        write!(f, "{}: {}", self.msg, self.path.display())
    }
}


// —————  MatchError  —————
/// Error messages about some path on the file system
#[derive(PartialEq, Debug)]
pub enum MatchError {
    /// No matching string found
    NoMatch,
    /// Multiple matching strings found; two examples follow
    MultiMatch(String, String),
}
impl ErrorTrait for MatchError {
    fn description(&self) -> &str {
        match self {
            &MatchError::NoMatch => "no matching string",
            &MatchError::MultiMatch(_,_) => "multiple matching strings",
        }
    }
}
impl fmt::Display for MatchError {
    fn fmt(&self, f: &mut fmt::Formatter) -> result::Result<(), fmt::Error> {
        match self {
            &MatchError::NoMatch => write!(f, "no match found"),
            &MatchError::MultiMatch(ref m1, ref m2) =>
                write!(f, "multiple matches found ({}, {}, ...)", m1, m2)
        }
    }
}


// —————  TipError  —————
/// Error type returned by `Partition::tip()`.
#[derive(PartialEq, Eq, Debug)]
pub enum TipError {
    /// Partition has not yet been loaded or set "new".
    NotReady,
    /// Loaded data left multiple tips. A merge is required to create a single
    /// tip.
    MergeRequired,
}
impl fmt::Display for TipError {
    fn fmt(&self, f: &mut fmt::Formatter) -> result::Result<(), fmt::Error> {
        match self {
            &TipError::NotReady => write!(f, "tip not ready: no tips loaded"),
            &TipError::MergeRequired => write!(f, "tip not ready: merge required"),
        }
    }
}
impl ErrorTrait for TipError {
    fn description(&self) -> &str { "tip not ready" }
}


// —————  OtherError  —————
/// Unclassified, generally not recoverable errors
#[derive(PartialEq, Eq, Debug)]
pub struct OtherError {
    msg: &'static str,
}
impl OtherError {
    /// Create with a message
    pub fn new(msg: &'static str) -> OtherError {
        OtherError { msg: msg }
    }
    /// New instance, wrapped with `Err`
    pub fn err<T>(msg: &'static str) -> Result<T> {
        Err(box OtherError::new(msg))
    }
}
impl fmt::Display for OtherError {
    fn fmt(&self, f: &mut fmt::Formatter) -> result::Result<(), fmt::Error> {
        write!(f, "{}", self.msg)
    }
}
impl ErrorTrait for OtherError {
    fn description(&self) -> &str { self.msg }
}

/// Use io::error::new to make an IO error
// #0011: replace all usages with Pippin-specific error types?
pub fn make_io_err<T>(kind: io::ErrorKind, msg: &'static str) -> Result<T> {
    Err(box io::Error::new(kind, msg))
}
