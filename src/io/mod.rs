/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Pippin: I/O traits

use std::io::{Read, Write};
use std::fmt::Debug;

use error::Result;

pub mod discover;
pub mod file;


/// An interface providing read and/or write access to a suitable location.
/// 
/// Note: lifetimes on some functions are more restrictive than might seem
/// necessary; this is to allow an implementation which reads and writes to
/// internal streams.
pub trait RepoIO: Debug {
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
    /// Convention: snapshot "zero" is written with no elements when the partition is first
    /// created. It can therefore be assumed that `ss_len` is at least 1, outside of
    /// `Partition::create`.
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
    fn append_ss_cl<'a>(&'a mut self, ss_num: usize, cl_num: usize) ->
            Result<Option<Box<Write+'a>>>;
    
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
/// method to save your data. Write operations succeed but forget the data.
#[derive(Debug, Default)]
pub struct DummyRepoIO {
    // The internal buffer allows us to accept write operations. Data gets
    // written over on the next write.
    buf: Vec<u8>
}
impl DummyRepoIO {
    /// Create a new instance
    pub fn new() -> DummyRepoIO {
        DummyRepoIO { buf: Vec::new() }
    }
}

impl RepoIO for DummyRepoIO {
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
    fn append_ss_cl<'a>(&'a mut self, _ss_num: usize, _cl_num: usize) ->
            Result<Option<Box<Write+'a>>>
    {
        self.buf.clear();
        Ok(Some(Box::new(&mut self.buf)))
    }
    fn new_ss_cl<'a>(&'a mut self, _ss_num: usize, _cl_num: usize) ->
            Result<Option<Box<Write+'a>>>
    {
        self.buf.clear();
        Ok(Some(Box::new(&mut self.buf)))
    }
}

impl RepoIO for Box<RepoIO> {
    fn ss_len(&self) -> usize { (**self).ss_len() }
    fn ss_cl_len(&self, ss_num: usize) -> usize { (**self).ss_cl_len(ss_num) }
    fn has_ss(&self, ss_num: usize) -> bool { (**self).has_ss(ss_num) }
    fn read_ss<'a>(&'a self, ss_num: usize) -> Result<Option<Box<Read+'a>>> {
        (**self).read_ss(ss_num)
    }
    fn read_ss_cl<'a>(&'a self, ss_num: usize, cl_num: usize) -> Result<Option<Box<Read+'a>>> {
        (**self).read_ss_cl(ss_num, cl_num)
    }
    fn new_ss<'a>(&'a mut self, ss_num: usize) -> Result<Option<Box<Write+'a>>> {
        (**self).new_ss(ss_num)
    }
    fn append_ss_cl<'a>(&'a mut self, ss_num: usize, cl_num: usize) ->
            Result<Option<Box<Write+'a>>>
    {
        (**self).append_ss_cl(ss_num, cl_num)
    }
    fn new_ss_cl<'a>(&'a mut self, ss_num: usize, cl_num: usize) -> Result<Option<Box<Write+'a>>>
    {
        (**self).new_ss_cl(ss_num, cl_num)
    }
}
