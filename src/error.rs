//! Internal error structs used by Pippin

use std::{io, error, fmt, result};

/// Our custom result type
pub type Result<T> = result::Result<T, Error>;

/// Our custom compound error type
pub enum Error {
    Read(ReadError),
    Io(io::Error)
}

/// For read errors; adds a read position
pub struct ReadError {
    msg: &'static str,
    pos: usize
}

impl Error {
    pub fn read(msg: &'static str, pos: usize) -> Error {
        Error::Read(ReadError { msg: msg, pos: pos })
    }
}

// Important impls for compound type
impl error::Error for Error {
    fn description(&self) -> &str {
        match(*self) {
            Error::Read(ref e) => e.msg,
            Error::Io(ref e) => e.description()
        }
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> result::Result<(), fmt::Error> {
        match(*self) {
            Error::Read(ref e) => { write!(f, "Position {}: {}", e.pos, e.msg); Ok(()) },
            Error::Io(ref e) => e.fmt(f)
        }
    }
}
impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> result::Result<(), fmt::Error> {
        match(*self) {
            Error::Read(ref e) => { write!(f, "Position {}: {}", e.pos, e.msg); Ok(()) },
            Error::Io(ref e) => e.fmt(f)
        }
    }
}

// From impls
impl From<ReadError> for Error {
    fn from(e: ReadError) -> Error { Error::Read(e) }
}
impl From<io::Error> for Error {
    fn from(e: io::Error) -> Error { Error::Io(e) }
}
