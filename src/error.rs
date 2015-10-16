//! Internal error structs used by Pippin

use std::{io, error, fmt, result, string};
use byteorder;

/// Our custom result type
pub type Result<T> = result::Result<T, Error>;

/// Our custom compound error type
pub enum Error {
    Read(ReadError),
    Arg(ArgError),
    /// No element found for replacement/removal/retrieval
    NoEltFound(&'static str),
    Replay(ReplayError),
    Io(io::Error),
    Utf8(string::FromUtf8Error),
}

/// For read errors; adds a read position
pub struct ReadError {
    msg: &'static str,
    pos: usize
}

/// Any error where an invalid argument was supplied
pub struct ArgError {
    msg: &'static str
}

/// Errors in log replay (due either to corruption or providing incompatible
/// states and commit logs)
pub struct ReplayError {
    msg: &'static str
}

impl Error {
    /// Create a "read" error with read position
    pub fn read(msg: &'static str, pos: usize) -> Error {
        Error::Read(ReadError { msg: msg, pos: pos })
    }
    /// Create an "invalid argument" error
    pub fn arg(msg: &'static str) -> Error {
        Error::Arg(ArgError { msg: msg })
    }
    /// Create a "no element found" error
    pub fn no_elt(msg: &'static str) -> Error {
        Error::NoEltFound(msg)
    }
    /// Create a "log replay" error
    pub fn replay(msg: &'static str) -> Error {
        Error::Replay(ReplayError { msg: msg })
    }
}

// Important impls for compound type
impl error::Error for Error {
    fn description(&self) -> &str {
        match *self {
            Error::Read(ref e) => e.msg,
            Error::Arg(ref e) => e.msg,
            Error::NoEltFound(msg) => msg,
            Error::Replay(ref e) => e.msg,
            Error::Io(ref e) => e.description(),
            Error::Utf8(ref e) => e.description(),
        }
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> result::Result<(), fmt::Error> {
        match *self {
            Error::Read(ref e) => write!(f, "Position {}: {}", e.pos, e.msg),
            Error::Arg(ref e) => write!(f, "Invalid argument: {}", e.msg),
            Error::NoEltFound(msg) => write!(f, "{}", msg),
            Error::Replay(ref e) => write!(f, "Failed to recreate state from log: {}", e.msg),
            Error::Io(ref e) => e.fmt(f),
            Error::Utf8(ref e) => e.fmt(f),
        }
    }
}
impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> result::Result<(), fmt::Error> {
        match *self {
            Error::Read(ref e) => write!(f, "Position {}: {}", e.pos, e.msg),
            Error::Arg(ref e) => write!(f, "Invalid argument: {}", e.msg),
            Error::NoEltFound(msg) => write!(f, "{}", msg),
            Error::Replay(ref e) => write!(f, "Failed to recreate state from log: {}", e.msg),
            Error::Io(ref e) => e.fmt(f),
            Error::Utf8(ref e) => e.fmt(f),
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
impl From<ReplayError> for Error {
    fn from(e: ReplayError) -> Error { Error::Replay(e) }
}
impl From<io::Error> for Error {
    fn from(e: io::Error) -> Error { Error::Io(e) }
}
impl From<string::FromUtf8Error> for Error {
    fn from(e: string::FromUtf8Error) -> Error { Error::Utf8(e) }
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