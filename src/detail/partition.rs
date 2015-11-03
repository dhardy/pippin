//! Pippin: partition

use std::io::{Read, Write};
use std::collections::HashSet;
use hashindexed::HashIndexed;

use super::{Sum, Commit, CommitQueue, LogReplay,
    PartitionState, PartitionStateSumComparator};
use super::{FileHeader, FileType, read_head, write_head,
    read_snapshot, write_snapshot, read_log, start_log, write_commit};
use error::{Result};

/// An interface providing read and/or write access to a suitable location.
pub trait PartitionIO {
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
    
    /// Get a snapshot with the given number. If no snapshot is present or if
    /// ss_num is too large, None will be returned.
    /// 
    /// This can fail due to IO operations failing.
    fn read_ss(&self, ss_num: usize) -> Result<Option<Box<Read>>>;
    
    /// Get a commit log (numbered `cl_num`) file for a snapshot (numbered
    /// `ss_num`). If none is found, return Ok(None).
    fn read_ss_cl(&self, ss_num: usize, cl_num: usize) -> Result<Option<Box<Read>>>;
    
    /// Open a write stream on a new snapshot file, numbered ss_num.
    /// This will increase the number returned by ss_len().
    /// 
    /// Fails if a snapshot with number ss_num already exists.
    fn new_ss(&mut self, ss_num: usize) -> Result<Box<Write>>;
    
    /// Open an append-write stream on an existing commit file. Writes may be
    /// atomic. Each commit should be written via a single write operation.
    //TODO: verify atomicity of writes
    fn append_ss_cl(&mut self, ss_num: usize, cl_num: usize) -> Result<Box<Write>>;
    
    /// Open a write-stream on a new commit file. As with the append version,
    /// the file will be opened in append mode, thus writes may be atomic.
    /// Each commit (and the header, including commit section marker) should be
    /// written via a single write operation.
    /// 
    /// Fails if a commit log with number `cl_num` for snapshot `ss_num`
    /// already exists.
    //TODO: verify atomicity of writes
    fn new_ss_cl(&mut self, ss_num: usize, cl_num: usize) -> Result<Box<Write>>;
    
    // TODO: other operations (delete file, extend commit log, ...?)
}

/// A *partition* is a sub-set of the entire set such that (a) each element is
/// in exactly one partition, (b) a partition is small enough to be loaded into
/// memory in its entirety, (c) there is some user control over the number of
/// partitions and how elements are assigned partitions and (d) each partition
/// can be managed independently of other partitions.
///
/// Partitions are the *only* method by which the entire set may grow beyond
/// available memory, thus smart allocation of elements to partitions will be
/// essential for some use-cases.
pub struct Partition {
    // IO provider
    io: Box<PartitionIO>,
    // Known committed states indexed by statesum 
    states: HashIndexed<PartitionState, Sum, PartitionStateSumComparator>,
    // All states without a known successor
    tips: HashSet<Sum>,
    // Commits created but not yet saved to disk
    unsaved: Vec<Commit>,
    // Index of parent state (i.e. the most recent commit). This is used when
    // making a new commit.
    // Special case: this is zero when there is no parent state (i.e. empty).
    parent: Sum,
    /// Current state. This may be equivalent to the parent state but will be a
    /// clone so that we can edit it directly without affecting the parent.
    /// 
    /// You can use this directly.
    pub cur: PartitionState,
}

// Methods creating a partition and loading its data
impl Partition {
    /// Create a new, empty partition. Note that repository partitioning is
    /// automatic, so the only reason you would want to do this would be to
    /// create a new single-partition "repository".
    /// 
    /// `io` must be passed in order to support saving to disk.
    pub fn new(io: Box<PartitionIO>) -> Partition {
        let mut states = HashIndexed::new();
        states.insert(PartitionState::new()); /* initial state */
        let mut tips = HashSet::new();
        tips.insert(Sum::zero() /* key of initial state */);
        Partition {
            io: io,
            states: states,
            tips: tips,
            unsaved: Vec::new(),
            parent: Sum::zero() /* key to initial state */,
            cur: PartitionState::new() /* copy of initial state */,
        }
    }
    
    /// Load the latest known state of a partition, using the latest available
    /// snapshot and all subsequent commit logs.
    pub fn load_latest(&mut self) -> Result<()> {
        let ss_len = self.io.ss_len();
        let mut num = ss_len - 1;
        
        loop {
            if let Some(mut r) = try!(self.io.read_ss(num)) {
                try!(self.validate_header(try!(read_head(&mut r))));
                let state = try!(read_snapshot(&mut r));
                self.tips.insert(state.statesum());
                self.states.insert(state);
                break;  // we stop at the most recent snapshot we find
            }
            if num == 0 { break;        /* no more to try; assume zero is empty state */ }
            num -= 1;
        }
        
        let mut queue = CommitQueue::new();
        for ss in num..ss_len {
            for c in 0..self.io.ss_cl_len(ss) {
                if let Some(mut r) = try!(self.io.read_ss_cl(ss, c)) {
                    try!(self.validate_header(try!(read_head(&mut r))));
                    try!(read_log(&mut r, &mut queue));
                }
            }
        }
        
        let mut replayer = LogReplay::from_sets(&mut self.states, &mut self.tips);
        try!(replayer.replay(queue));
        
        Ok(())
    }
    
    /// Load all history available
    pub fn load_everything(&mut self) -> Result<()> {
        let ss_len = self.io.ss_len();
        
        for num in 0..ss_len {
            if let Some(mut r) = try!(self.io.read_ss(num)) {
                try!(self.validate_header(try!(read_head(&mut r))));
                let state = try!(read_snapshot(&mut r));
                self.tips.insert(state.statesum());
                self.states.insert(state);
            }
            
            let mut queue = CommitQueue::new();
            for c in 0..self.io.ss_cl_len(num) {
                if let Some(mut r) = try!(self.io.read_ss_cl(num, c)) {
                    try!(self.validate_header(try!(read_head(&mut r))));
                    try!(read_log(&mut r, &mut queue));
                }
            }
            let mut replayer = LogReplay::from_sets(&mut self.states, &mut self.tips);
            try!(replayer.replay(queue));
        }
        
        Ok(())
    }
    
    /// Accept a header, and check that the file corresponds to this repository
    /// and partition. If this `Partition` is new, it is instead assigned to
    /// the repository and partition identified by the file. This function is
    /// called for every file loaded.
    pub fn validate_header(&mut self, _header: FileHeader) -> Result<()> {
        // TODO: I guess we need to assign repository and partition UUIDs or
        // something, and consider how to handle history across repartitioning.
        Ok(())
    }
}

// Methods saving a partition's data
impl Partition {
    /// Commit changes to the log in memory and optionally on the disk.
    /// 
    /// In all modes, this will create a new commit in memory (if there are any
    /// changes to commit).
    /// 
    /// If mode is at least `CommitMode::FastWrite`, then any pending changes
    /// will be written to the disk.
    /// 
    /// Finally, if mode is `CommitMode::Write`, any maintenance operations
    /// required will be done (for example creating a new snapshot when the
    /// current commit log is too long).
    /// 
    /// Note that writing to disk can fail. In this case it may be worth trying
    /// again
    pub fn commit(&mut self, mode: CommitMode) -> Result<()> {
        // First step: make a new commit
        // TODO: diff-based commit?
        if self.parent == Sum::zero() {
            if !self.cur.is_empty() {
                self.states.insert(self.cur.clone());
                self.unsaved.push(Commit::from_diff(&PartitionState::new(), &self.cur));
            }
        } else {
            let new_commit = if let Some(state) = self.states.get(&self.parent) {
                if self.cur != *state {
                    Some(Commit::from_diff(state, &self.cur))
                } else {
                    None
                }
            } else {
                //TODO: should we panic? In this case either there is in-memory
                // corruption or there is a code error. It is doubtful we can recover.
                panic!("Partition: state not found!");
            };
            // If we have a new commit, insert now that the borrow on self.states has expired.
            if let Some(commit) = new_commit {
                self.unsaved.push(commit);
                self.states.insert(self.cur.clone());
            }
        }
        if mode == CommitMode::InMem { return Ok(()); }
        
        // Second step: write commits
        if !self.unsaved.is_empty() {
            // TODO: we need a proper commit policy!
            let ss_num = self.io.ss_len() - 1;  // assume commit number (TODO: this is wrong)
            let cl_num = self.io.ss_cl_len(ss_num);     // new commit (TODO don't always want this)
            let (mut w,is_new) = (try!(self.io.new_ss_cl(ss_num, cl_num)), true);
            if is_new {
                let header = FileHeader {
                    ftype: FileType::CommitLog,
                    name: "".to_string() /*TODO: repo name?*/,
                    remarks: Vec::new(),
                    user_fields: Vec::new(),
                };
                try!(write_head(&header, &mut w));
                try!(start_log(&mut w));
            };
            while !self.unsaved.is_empty() {
                // We try to write the commit, then when successful remove it
                // from the list of 'unsaved' commits.
                try!(write_commit(&self.unsaved[self.unsaved.len()-1], &mut w));
                self.unsaved.pop();
            }
        }
        if mode == CommitMode::FastWrite { return Ok(()); }
        
        // Third step: maintenance operations
        if false /*TODO new_snapshot_needed*/ {
            try!(self.write_snapshot());
        }
        assert_eq!( mode, CommitMode::Write );
        Ok(())
    }
    
    /// Write a new snapshot from the latest commit, unless the partition has
    /// no commits, in which case do nothing.
    //TODO: writing a new snapshot is only acceptable when we are currently on
    // the latest state, otherwise what was the latest state will be "hidden"
    // (only the latest snapshot is read normally).
    // Make partition read-only until latest state is read?
    pub fn write_snapshot(&mut self) -> Result<()> {
        match self.states.get(&self.parent) {
            None => { Ok(()) },
            Some(state) => {
                //TODO: we should try a new snapshot number if this fails with AlreadyExists
                let ss_num = self.io.ss_len();
                let mut writer = try!(self.io.new_ss(ss_num));
                write_snapshot(state, &mut writer)
            }
        }
    }
}

#[derive(Eq, PartialEq, Debug)]
enum CommitMode {
    // Create a commit but don't save it to the disk
    InMem,
    // Create and write, but don't do any long maintenance operations
    FastWrite,
    // Create, write and do any required maintenance operations
    Write
}
