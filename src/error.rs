/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Internal error structs used by Pippin

use std::{io, fmt, result};
use std::path::PathBuf;
use std::cmp::{min, max};

use util::HexFormatter;

/// Our custom result type
pub type Result<T, E = Error> = result::Result<T, E>;

/// Our custom compound error type
pub type Error = Box<ErrorTrait>;

use std::error::Error as ErrorTrait;


// —————  ReadError  —————

/// This is a variant of the core `try!(...)` macro which adds position data
/// in a stream to the error, on failure. Return type is `Result<_, ReadError>`.
/// 
/// Usage: `try_read!(expr, pos, offset)`. See documentation of `ReadError::new` for details.
#[macro_export]
macro_rules! try_read {
    ($expr:expr, $pos:expr, $offset:expr) => (match $expr {
        Ok(val) => val,
        Err(err) => {{
            return Err(ReadError::new_wrap(err.into(), $pos, $offset));
        }}
    })
}

#[derive(Debug)]
enum Wrapped {
    Msg(&'static str),
    ErrT(Error),
}
/// For read errors; adds a read position
#[derive(Debug)]
pub struct ReadError {
    detail: Wrapped,
    pos: usize,
    off_start: usize,
    off_end: usize,
}
impl ReadError {
    /// Create a "read" error with read position
    /// 
    /// `pos`: the start of the text to display (length is determined
    /// automatically from `offset`, rounded up to eight-byte blocks).
    /// 
    /// `offset`: the region of the displayed text to highlight.
    pub fn new(msg: &'static str, pos: usize, offset: (usize, usize)) -> ReadError {
        let (o0, o1) = offset;
        ReadError { detail: Wrapped::Msg(msg), pos: pos, off_start: o0, off_end: o1 }
    }
    /// New instance, wrapped with `Err` (see `new()`).
    pub fn err<T>(msg: &'static str, pos: usize, offset: (usize, usize)) -> Result<T> {
        Err(Box::new(ReadError::new(msg, pos, offset)))
    }
    /// Create a "read" error wrapping another error
    pub fn new_wrap(e: Error, pos: usize, offset: (usize, usize)) -> ReadError {
        let (o0, o1) = offset;
        ReadError { detail: Wrapped::ErrT(e), pos: pos, off_start: o0, off_end: o1 }
    }
    /// Return an object which can be used in format expressions.
    /// 
    /// Usage: `println!("{}", err.display(&buf));`
    pub fn display<'a>(&'a self, data: &'a [u8]) -> ReadErrorFormatter<'a> {
        ReadErrorFormatter { err: self, data: data }
    }
}
impl ErrorTrait for ReadError {
    fn description(&self) -> &str {
        match self.detail {
            Wrapped::Msg(msg) => msg,
            Wrapped::ErrT(ref e) => e.description(),
        }
    }
}
impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter) -> result::Result<(), fmt::Error> {
        write!(f, "read error at position {}, offset ({}, {}): ", 
                self.pos, self.off_start, self.off_end)?;
        match self.detail {
            Wrapped::Msg(msg) => write!(f, "{}", msg),
            Wrapped::ErrT(ref e) => e.fmt(f),
        }
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
        
        write!(f, "read error (pos {}, offset ({}, {})): ", self.err.pos,
            self.err.off_start, self.err.off_end)?;
        match self.err.detail {
            Wrapped::Msg(msg) => write!(f, "{}", msg),
            Wrapped::ErrT(ref e) => e.fmt(f),
        }?;
        let start = self.err.pos + 8 * (self.err.off_start / 8);
        let end = self.err.pos + 8 * ((self.err.off_end + 7) / 8);
        // #0018: we could use for line_start in (start..end).step_by(8) once
        // Rust issue #27741 is closed.
        let mut line_start = start;
        while line_start < end {
            if line_start + 8 > self.data.len() {
                writeln!(f, "insufficient data to display!")?;
                break;
            }
            HexFormatter::line(&self.data[line_start..line_start+8]).fmt(f)?;
            let p0 = max(self.err.pos + self.err.off_start, line_start) - line_start;
            let p1 = min(self.err.pos + self.err.off_end, line_start + 8) - line_start;
            assert!(p0 <= p1 && p1 <= 8);
            write!(f, "{}{}{}", &SPACE[0..(3*p0)], &MARK[(3*p0)..(3*p1-1)], &SPACE[(3*p1-1)..24])?;
            writeln!(f, "{}{}", &SPACE[0..p0], &MARK[p0..p1])?;
            
            line_start += 8;
        }
        Ok(())
    }
}


// —————  ArgError  ————
/// Any error where an invalid argument was supplied
#[derive(PartialEq, Eq, Debug)]
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
        Err(Box::new(ArgError::new(msg)))
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
    EltNotFound,
    /// Unable to find a free element identifier for a new element
    IdGenFailure,
    /// Wrong partition identifier. This should only be returned for operations
    /// on a specified partition, and indicates that the given identifier is
    /// from another partition.
    WrongPartId,
    /// Identifier already in use. An insertion failed since the given
    /// identifier is already in use.
    IdClash,
    /// Unable to proceed because classification failed
    ClassifyFailure,
    /// The relevant partition could not be found
    PartNotFound,
}
impl ErrorTrait for ElementOp {
    fn description(&self) -> &'static str {
        match *self {
            ElementOp::EltNotFound => "element not found",
            ElementOp::IdGenFailure => "id generation failed to find a free identifier",
            ElementOp::WrongPartId => "operation on a partition uses element identifier from another partition",
            ElementOp::IdClash => "identifier already in use",
            ElementOp::ClassifyFailure => "classification of element failed",
            ElementOp::PartNotFound => "partition not found or not loaded",
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
/// Any `ElementOp` can automatically be converted to `PatchOp::PatchApply`.
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


// —————  PathError  —————
/// Error messages about some path on the file system
#[derive(PartialEq, Eq, Debug)]
pub struct PathError {
    msg: &'static str,
    path: PathBuf,
}
impl PathError {
    /// Create a "path" error. Will be displayed as
    /// `println!("Error: {}: {}", msg, path.display());`.
    pub fn new<P: Into<PathBuf>>(msg: &'static str, path: P) -> PathError {
        PathError { msg: msg, path: path.into() }
    }
    /// New instance, wrapped with `Err`
    pub fn err<T, P: Into<PathBuf>>(msg: &'static str, path: P) -> Result<T> {
        Err(Box::new(PathError::new(msg, path)))
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
#[derive(PartialEq, Eq, Debug)]
pub enum MatchError {
    /// No matching string found
    NoMatch,
    /// Multiple matching strings found; two examples follow
    MultiMatch(String, String),
}
impl ErrorTrait for MatchError {
    fn description(&self) -> &str {
        match *self {
            MatchError::NoMatch => "no matching string",
            MatchError::MultiMatch(_,_) => "multiple matching strings",
        }
    }
}
impl fmt::Display for MatchError {
    fn fmt(&self, f: &mut fmt::Formatter) -> result::Result<(), fmt::Error> {
        match *self {
            MatchError::NoMatch => write!(f, "no match found"),
            MatchError::MultiMatch(ref m1, ref m2) =>
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
impl ErrorTrait for TipError {
    fn description(&self) -> &str {
        match *self {
            TipError::NotReady => "tip not ready: no tips loaded",
            TipError::MergeRequired => "tip not ready: merge required",
        }
    }
}
impl fmt::Display for TipError {
    fn fmt(&self, f: &mut fmt::Formatter) -> result::Result<(), fmt::Error> {
        write!(f, "{}", self.description())
    }
}


// —————  MergeError  —————
/// Error type returned when a merge fails
#[derive(PartialEq, Eq, Debug)]
pub enum MergeError {
    /// One of the states to be merged was not found
    NoState,
    /// No common ancestor found
    NoCommonAncestor,
    /// Solver did not find a solution
    NotSolved,
    /// Patching failed
    PatchOp(PatchOp),
}
impl ErrorTrait for MergeError {
    fn description(&self) -> &str {
        match *self {
            MergeError::NoState => "merge: could not find state",
            MergeError::NoCommonAncestor => "merge: could not find a common ancestor",
            MergeError::NotSolved => "merge: solver failed",
            MergeError::PatchOp(ref p) => p.description(),
        }
    }
}
impl fmt::Display for MergeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> result::Result<(), fmt::Error> {
        write!(f, "{}", self.description())
    }
}
impl From<PatchOp> for MergeError {
    fn from(e: PatchOp) -> MergeError {
        MergeError::PatchOp(e)
    }
}


// —————  ReadOnly  —————
/// Thing is not modifiable.
#[derive(PartialEq, Eq, Debug)]
pub struct ReadOnly {}
impl ReadOnly {
    /// Create.
    pub fn new() -> ReadOnly { ReadOnly{} }
    /// Create, wrapped with `Err`
    pub fn err<T>() -> Result<T> {
        Err(Box::new(ReadOnly::new()))
    }
}
impl ErrorTrait for ReadOnly {
    fn description(&self) -> &str {
        "operation failed: readonly"
    }
}
impl fmt::Display for ReadOnly {
    fn fmt(&self, f: &mut fmt::Formatter) -> result::Result<(), fmt::Error> {
        write!(f, "operation failed: readonly")
    }
}


// —————  UserError  —————
/// An error the user may return
#[derive(PartialEq, Eq, Debug)]
pub struct UserError {
    /// Arbitrary code a user may set
    pub code: u64,
    /// Message to display
    pub msg: &'static str,
}
impl UserError {
    /// Create
    pub fn new(code: u64, msg: &'static str) -> UserError {
        UserError { code: code, msg: msg }
    }
}
impl fmt::Display for UserError {
    fn fmt(&self, f: &mut fmt::Formatter) -> result::Result<(), fmt::Error> {
        write!(f, "UserError (code {}): {}", self.code, self.msg)
    }
}
impl ErrorTrait for UserError {
    fn description(&self) -> &str { self.msg }
}


// —————  ClassifyError  —————
/// Failures during classification
#[derive(Debug)]
pub enum ClassifyError {
    /// Property unknown or unavailable
    UnknownProperty,
    /// No partition matches the given element
    NoPartMatches,
}
impl ErrorTrait for ClassifyError {
    fn description(&self) -> &str {
        match *self {
            ClassifyError::UnknownProperty => "classify: property unknown or unavailable",
            ClassifyError::NoPartMatches => "classify: no matching partition found",
        }
    }
}
impl fmt::Display for ClassifyError {
    fn fmt(&self, f: &mut fmt::Formatter) -> result::Result<(), fmt::Error> {
        write!(f, "{}", self.description())
    }
}
impl From<ClassifyError> for ElementOp {
    fn from(_: ClassifyError) -> ElementOp {
        ElementOp::ClassifyFailure
    }
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
        Err(Box::new(OtherError::new(msg)))
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

/// Use `io::error::new` to make an IO error
// #0011: replace all usages with Pippin-specific error types?
pub fn make_io_err<T>(kind: io::ErrorKind, msg: &'static str) -> Result<T> {
    Err(Box::new(io::Error::new(kind, msg)))
}
