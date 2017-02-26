/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Pippin: partition traits

use std::any::Any;
use std::io::{Read, Write};

use {FileHeader, Result, UserData};
use commit::{MakeCommitMeta};


/// Allows the user to control various partition operations. Library-provided implementations
/// should be sufficient for many use-cases, but can be overridden or replaced if necessary.
/// 
/// Pippin data files allow arbitrary *user fields* in the headers; these can be set and read on
/// file creation / loading.
/// 
/// Each commit carries metadata: a timestamp and an "extra metadata" field; these can be set
/// by the user. (They can be read by retrieving and examining a `Commit`).
pub trait UserPartT: MakeCommitMeta {
    /// Convert self to a `&Any`
    fn as_any(&self) -> &Any;
    
    /// Get access to an I/O provider.
    /// 
    /// This layer of indirection allows use of `PartFileIO`, which should be sufficient
    /// for many use cases.
    fn io<'a>(&'a self) -> &'a PartIO;
    
    /// Get mutable access to an I/O provider.
    fn io_mut<'a>(&'a mut self) -> &'a mut PartIO;
    
    /// Get access to the snapshot policy.
    /// 
    /// This layer of indirection allows use of the `DefaultSnapshot`.
    fn snapshot_policy(&mut self) -> &mut SnapshotPolicy;
    
    /// Cast self to a `&MakeCommitMeta`
    // #0018: shouldn't be needed when Rust finally supports upcasting
    fn as_mcm_ref(&self) -> &MakeCommitMeta;
    
    /// Cast self to a `&mut MakeCommitMeta`
    // #0018: shouldn't be needed when Rust finally supports upcasting
    fn as_mcm_ref_mut(&mut self) -> &mut MakeCommitMeta;
    
    // #0040: inform user of file name and/or SS&CL numbers when reading/writing user data
    
    /// This function allows population of the *user fields* of a header. This function is passed
    /// a reference to a `FileHeader` struct, where all fields have been set excepting `user`,
    /// the user fields (this should be an empty container). This function should return a set of
    /// user data to be added to the `FileHeader`.
    /// 
    /// The partition identifier and file type can be read from the passed `FileHeader`.
    /// 
    /// Returning an error will abort creation of the corresponding file.
    /// 
    /// The default implementation does not make any user data (returns an empty `Vec`).
    fn make_user_data(&mut self, _header: &FileHeader) -> Result<Vec<UserData>> {
        Ok(vec![])
    }
    
    /// This function allows the user to read data from a header when a file is loaded.
    /// 
    /// Returning an error will abort reading of this file.
    /// 
    /// The default implementation does nothing.
    fn read_header(&mut self, _header: &FileHeader) -> Result<()> {
        Ok(())
    }
}

/// A convenient implementation of `UserPartT`.
/// 
/// Uses `DefaultSnapshot` snapshot policy.
#[derive(Debug)]
pub struct DefaultUserPartT<IO: PartIO + 'static> {
    io: IO,
    ss_policy: DefaultSnapshot,
}
impl<IO: PartIO> DefaultUserPartT<IO> {
    /// Create, given I/O provider
    pub fn new(io: IO) -> Self {
        DefaultUserPartT { io: io, ss_policy: Default::default() }
    }
}
impl<IO: PartIO> MakeCommitMeta for DefaultUserPartT<IO> {}
impl<IO: PartIO> UserPartT for DefaultUserPartT<IO> {
    fn as_any(&self) -> &Any { self }
    fn io<'a>(&'a self) -> &'a PartIO {
        &self.io
    }
    fn io_mut<'a>(&'a mut self) -> &'a mut PartIO {
        &mut self.io
    }
    fn snapshot_policy(&mut self) -> &mut SnapshotPolicy {
        &mut self.ss_policy
    }
    fn as_mcm_ref(&self) -> &MakeCommitMeta { self }
    fn as_mcm_ref_mut(&mut self) -> &mut MakeCommitMeta { self }
}

/// An interface allowing configuration of snapshot policy.
/// 
/// It is assumed that one or more internal counters are incremented when `count` is called and
/// used to determine when `want_snapshot` returns true.
pub trait SnapshotPolicy {
    /// Reset internal counters (we have an up-to-date snapshot).
    fn reset(&mut self);
    
    /// Declare that a snapshot is required (i.e. force `want_snapshot` to be true until `reset` is
    /// next called).
    fn force_snapshot(&mut self);
    
    /// Increment an internal counter/counters to record this many `commits` and `edits`.
    fn count(&mut self, commits: usize, edits: usize);
    
    /// Defines our snapshot policy: this should return true when a new snapshot is required.
    /// 
    /// The number of commits and edits saved since the last snapshot 
    /// The default implementation is
    /// ```rust
    /// commits * 5 + edits > 150
    /// ```
    fn want_snapshot(&self) -> bool;
}

/// Default snapshot policy: snapshot when `commits * 5 + edits > 150`.
/// 
/// Can be constructed with `Default`.
#[derive(Debug, Default)]
pub struct DefaultSnapshot {
    counter: usize,
}

impl SnapshotPolicy for DefaultSnapshot {
    fn reset(&mut self) {
        self.counter = 0;
    }
    
    fn force_snapshot(&mut self) {
        self.counter = 1000;
    }
    
    fn count(&mut self, commits: usize, edits: usize) {
        self.counter += commits * 5 + edits;
    }
    
    fn want_snapshot(&self) -> bool {
        self.counter > 150
    }
}

/// An interface providing read and/or write access to a suitable location.
/// 
/// Note: lifetimes on some functions are more restrictive than might seem
/// necessary; this is to allow an implementation which reads and writes to
/// internal streams.
pub trait PartIO {
    /// Convert self to a `&Any`
    fn as_any(&self) -> &Any;
    
    /// Return one greater than the snapshot number of the latest snapshot file
    /// or log file found.
    /// 
    /// The idea is that each snapshot and each set of log files can be put
    /// into a sparse vector with this length (sparse because entries may be
    /// missing; especially old entries may have been deleted).
    /// 
    /// Snapshots and commit logs with a number greater than or equal to this
    /// number probably won't exist and may in any case be ignored.
    /// 
    /// Convention: snapshot "zero" may not be an actual snapshot but
    /// either way the snapshot should be empty (no elements and the state-sum
    /// should be zero).
    /// 
    /// This number must not change except to increase when write_snapshot()
    /// is called.
    fn ss_len(&self) -> usize;
    
    /// One greater than the number of the last log file available for some snapshot
    fn ss_cl_len(&self, ss_num: usize) -> usize;
    
    /// Tells whether a snapshot file with this number is available. If true,
    /// `read_ss(ss_num)` *should* succeed (assuming no I/O failure).
    fn has_ss(&self, ss_num: usize) -> bool;
    
    /// Get a snapshot with the given number. If no snapshot is present or if
    /// ss_num is too large, None will be returned.
    /// 
    /// Returns a heap-allocated read stream, either on some external resource
    /// (such as a file) or on an internal data-structure.
    /// 
    /// This can fail due to IO operations failing.
    fn read_ss<'a>(&'a self, ss_num: usize) -> Result<Option<Box<Read+'a>>>;
    
    /// Get a commit log (numbered `cl_num`) file for a snapshot (numbered
    /// `ss_num`). If none is found, return Ok(None).
    /// 
    /// Returns a heap-allocated read stream, either on some external resource
    /// (such as a file) or on an internal data-structure.
    /// 
    /// This can fail due to IO operations failing.
    fn read_ss_cl<'a>(&'a self, ss_num: usize, cl_num: usize) -> Result<Option<Box<Read+'a>>>;
    
    /// Open a write stream on a new snapshot file, numbered ss_num.
    /// This will increase the number returned by ss_len().
    /// 
    /// Returns None if a snapshot with number ss_num already exists.
    /// 
    /// Returns a heap-allocated write stream, either to some external resource
    /// (such as a file) or to an internal data-structure.
    /// 
    /// This can fail due to IO operations failing.
    fn new_ss<'a>(&'a mut self, ss_num: usize) -> Result<Option<Box<Write+'a>>>;
    
    /// Open an append-write stream on an existing commit file. Writes may be
    /// atomic. Each commit should be written via a single write operation.
    /// 
    /// Returns None if no commit file with this `ss_num` and `cl_num` exists.
    /// 
    /// Returns a heap-allocated write stream, either to some external resource
    /// (such as a file) or to an internal data-structure.
    /// 
    /// This can fail due to IO operations failing.
    // #0012: verify atomicity of writes
    fn append_ss_cl<'a>(&'a mut self, ss_num: usize, cl_num: usize) -> Result<Option<Box<Write+'a>>>;
    
    /// Open a write-stream on a new commit file. As with the append version,
    /// the file will be opened in append mode, thus writes may be atomic.
    /// Each commit (and the header, including commit section marker) should be
    /// written via a single write operation.
    /// 
    /// Returns None if a commit log with number `cl_num` for snapshot `ss_num`
    /// already exists.
    /// 
    /// Returns a heap-allocated write stream, either to some external resource
    /// (such as a file) or to an internal data-structure.
    /// 
    /// This can fail due to IO operations failing.
    // #0012: verify atomicity of writes
    fn new_ss_cl<'a>(&'a mut self, ss_num: usize, cl_num: usize) -> Result<Option<Box<Write+'a>>>;
}

/// Doesn't provide any IO.
/// 
/// Can be used for testing but big fat warning: this does not provide any
/// method to save your data. Write operations fail with `ErrorKind::InvalidInput`.
#[derive(Debug)]
pub struct DummyPartIO {
    // The internal buffer allows us to accept write operations. Data gets
    // written over on the next write.
    buf: Vec<u8>
}
impl DummyPartIO {
    /// Create a new instance
    pub fn new() -> DummyPartIO {
        DummyPartIO { buf: Vec::new() }
    }
}

impl PartIO for DummyPartIO {
    fn as_any(&self) -> &Any { self }
    fn ss_len(&self) -> usize { 0 }
    fn ss_cl_len(&self, _ss_num: usize) -> usize { 0 }
    fn has_ss(&self, _ss_num: usize) -> bool { false }
    fn read_ss(&self, _ss_num: usize) -> Result<Option<Box<Read+'static>>> {
        Ok(None)
    }
    fn read_ss_cl(&self, _ss_num: usize, _cl_num: usize) -> Result<Option<Box<Read+'static>>> {
        Ok(None)
    }
    fn new_ss<'a>(&'a mut self, _ss_num: usize) -> Result<Option<Box<Write+'a>>> {
        self.buf.clear();
        Ok(Some(Box::new(&mut self.buf)))
    }
    fn append_ss_cl<'a>(&'a mut self, _ss_num: usize, _cl_num: usize) -> Result<Option<Box<Write+'a>>> {
        self.buf.clear();
        Ok(Some(Box::new(&mut self.buf)))
    }
    fn new_ss_cl<'a>(&'a mut self, _ss_num: usize, _cl_num: usize) -> Result<Option<Box<Write+'a>>> {
        self.buf.clear();
        Ok(Some(Box::new(&mut self.buf)))
    }
}
