//! Internal error structs used by Pippin

use std::{io, fmt, result};
use std::path::PathBuf;
use std::cmp::{min, max};

/// Our custom result type
pub type Result<T> = result::Result<T, Error>;

/// Our custom compound error type
pub type Error = Box<ErrorTrait>;

pub use std::error::Error as ErrorTrait;

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

/// Element operation error details
#[derive(PartialEq, Debug)]
pub struct ElementOp {
    /// Identity of element
    pub id: u64,
    /// Classification of failure
    pub class: ElementOpClass,
}
impl ErrorTrait for ElementOp {
    fn description(&self) -> &str { self.description() }
}

/// Classification of element operation failure
#[derive(PartialEq, Debug)]
pub enum ElementOpClass {
    /// Insertion failed due to identity clash (element identifier)
    InsertionFailure,
    /// Replacement failed due to missing element (element identifier)
    ReplacementFailure,
    /// Deletion failed due to missing element (element identifier)
    DeletionFailure,
}
impl ElementOp {
    /// Get the description string corresponding to the classification
    pub fn description(&self) -> &'static str {
        match self.class {
            ElementOpClass::InsertionFailure => "insertion failed: identifier already in use",
            ElementOpClass::ReplacementFailure => "replacement failed: cannot find element to replace",
            ElementOpClass::DeletionFailure => "deletion failed: element not found",
        }
    }
    /// Create an instance
    pub fn insertion_failure(id: u64) -> ElementOp {
        ElementOp { id: id, class: ElementOpClass::InsertionFailure }
    }
    /// Create an instance
    pub fn replacement_failure(id: u64) -> ElementOp {
        ElementOp { id: id, class: ElementOpClass::ReplacementFailure }
    }
    /// Create an instance
    pub fn deletion_failure(id: u64) -> ElementOp {
        ElementOp { id: id, class: ElementOpClass::DeletionFailure }
    }
}
impl fmt::Display for ElementOp {
    fn fmt(&self, f: &mut fmt::Formatter) -> result::Result<(), fmt::Error> {
        write!(f, "{}: {}", self.description(), self.id)
    }
}

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

/// Use io::error::new to make an IO error
//TODO: replace all usages with Pippin-specific error types?
pub fn make_io_err<T>(kind: io::ErrorKind, msg: &'static str) -> Result<T> {
    Err(box io::Error::new(kind, msg))
}
