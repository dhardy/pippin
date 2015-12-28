//! Internal error structs used by Pippin

use std::{io, error, fmt, result, string, num, env};
use std::path::PathBuf;
use std::cmp::{min, max};
use byteorder;
use regex;
use TipError;

/// Our custom result type
pub type Result<T> = result::Result<T, Error>;

/// Our custom compound error type
#[derive(Debug)]
pub enum Error {
    /// Error while reading from a stream (syntactical or semantic)
    Read(ReadError),
    /// Invalid argument
    Arg(ArgError),
    /// Element operation failure
    ElementOp(ElementOp),
    /// Regeneration of state from change sets failed
    Replay(ReplayError),
    /// Error messages about some path on the file system
    Path(&'static str, PathBuf),
    /// Loaded data is not ready for use
    NotReady(&'static str),
    /// An external command failed to run
    CmdFailed(String),
    /// Encapsulation of a standard library error
    Io(io::Error),
    /// Encapsulation of a standard library error
    Utf8(string::FromUtf8Error),
    /// Encapsulation of a standard library error
    ParseInt(num::ParseIntError),
    /// Encapsulation of a standard library error
    VarError(env::VarError),
    /// Encapsulation of a regex library error
    Regex(regex::Error),
}

/// For read errors; adds a read position
#[derive(PartialEq, Debug)]
pub struct ReadError {
    msg: &'static str,
    pos: usize,
    off_start: usize,
    off_end: usize,
}

impl ReadError {
    /// Return an object which can be used in format expressions.
    /// 
    /// Usage: `println!("{}", err.display(&buf));`
    pub fn display<'a>(&'a self, data: &'a [u8]) -> ReadErrorFormatter<'a> {
        ReadErrorFormatter { err: self, data: data }
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

/// Element operation error details
#[derive(PartialEq, Debug)]
pub struct ElementOp {
    /// Identity of element
    pub id: u64,
    /// Classification of failure
    pub class: ElementOpClass,
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

impl Error {
    /// Create a "read" error with read position
    pub fn read(msg: &'static str, pos: usize, offset: (usize, usize)) -> Error {
        let (o0, o1) = offset;
        Error::Read(ReadError { msg: msg, pos: pos, off_start: o0, off_end: o1 })
    }
    /// Create an "invalid argument" error
    pub fn arg(msg: &'static str) -> Error {
        Error::Arg(ArgError { msg: msg })
    }
    /// Create a "log replay" error
    pub fn replay(msg: &'static str) -> Error {
        Error::Replay(ReplayError { msg: msg })
    }
    /// Create a "path" error. Will be displayed as
    /// `println!("Error: {}: {}", msg, path.display());`.
    pub fn path(msg: &'static str, path: PathBuf) -> Error {
        Error::Path(msg, path)
    }
    /// Create an "external command" error.
    pub fn cmd_failed<T: fmt::Display>(cmd: T, status: Option<i32>) -> Error {
        Error::CmdFailed(match status {
            Some(code) => format!("Command failed with status {}: {}", code, cmd),
            None => format!("Command failed (interrupted): {}", cmd),
        })
    }
    /// Use io::error::new to make an IO error
    //TODO: replace all usages with Pippin-specific error types?
    pub fn io(kind: io::ErrorKind, msg: &'static str) -> Error {
        Error::Io(io::Error::new(kind, msg))
    }
}

// Important impls for compound type
impl error::Error for Error {
    fn description(&self) -> &str {
        match *self {
            Error::Read(ref e) => e.msg,
            Error::Arg(ref e) => e.msg,
            Error::ElementOp(ref e) => e.description(),
            Error::Replay(ref e) => e.msg,
            Error::Path(ref msg, _) => msg,
            Error::NotReady(ref msg) => msg,
            Error::CmdFailed(ref msg) => &msg,
            Error::Io(ref e) => e.description(),
            Error::Utf8(ref e) => e.description(),
            Error::ParseInt(ref e) => e.description(),
            Error::VarError(ref e) => e.description(),
            Error::Regex(ref e) => e.description(),
        }
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> result::Result<(), fmt::Error> {
        match *self {
            Error::Read(ref e) => write!(f, "read error at position {}, offset ({}, {}): {}", e.pos, e.off_start, e.off_end, e.msg),
            Error::Arg(ref e) => write!(f, "invalid argument: {}", e.msg),
            Error::ElementOp(ref e) => e.fmt(f),
            Error::Replay(ref e) => write!(f, "failed to recreate state from log: {}", e.msg),
            Error::Path(ref msg, ref path) => write!(f, "{}: {}", msg, path.display()),
            Error::NotReady(ref msg) => write!(f, "{}", msg),
            Error::CmdFailed(ref msg) => write!(f, "{}", msg),
            Error::Io(ref e) => e.fmt(f),
            Error::Utf8(ref e) => e.fmt(f),
            Error::ParseInt(ref e) => e.fmt(f),
            Error::VarError(ref e) => e.fmt(f),
            Error::Regex(ref e) => e.fmt(f),
        }
    }
}

// From impls
impl From<ReadError> for Error {
    fn from(e: ReadError) -> Error { Error::Read(e) }
}
impl From<ArgError> for Error {
    fn from(e: ArgError) -> Error { Error::Arg(e) }
}
impl From<ElementOp> for Error {
    fn from(e: ElementOp) -> Error { Error::ElementOp(e) }
}
impl From<ReplayError> for Error {
    fn from(e: ReplayError) -> Error { Error::Replay(e) }
}
impl From<io::Error> for Error {
    fn from(e: io::Error) -> Error { Error::Io(e) }
}
impl From<string::FromUtf8Error> for Error {
    fn from(e: string::FromUtf8Error) -> Error { Error::Utf8(e) }
}
impl From<num::ParseIntError> for Error {
    fn from(e: num::ParseIntError) -> Error { Error::ParseInt(e) }
}
impl From<env::VarError> for Error {
    fn from(e: env::VarError) -> Error { Error::VarError(e) }
}
impl From<byteorder::Error> for Error {
    fn from(e: byteorder::Error) -> Error {
        match e {
        //TODO (Rust 1.4): use io::ErrorKind::UnexpectedEOF instead of Other
            byteorder::Error::UnexpectedEOF =>
                Error::Io(io::Error::new(io::ErrorKind::Other, "unexpected EOF")),
            byteorder::Error::Io(err) => Error::Io(err)
        }
    }
}
impl From<regex::Error> for Error {
    fn from(e: regex::Error) -> Error { Error::Regex(e) }
}
impl From<TipError> for Error {
    fn from(e: TipError) -> Error { Error::NotReady(match e {
            TipError::NotReady => "partition not ready: no states found",
            TipError::MergeRequired => "partition not ready: merge required",
        })
    }
}
