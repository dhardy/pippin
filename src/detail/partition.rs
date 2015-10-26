//! Pippin: partition

use std::io::{Read, Write};
use std::collections::HashSet;
use hashindexed::HashIndexed;

use super::{Sum, Commit, CommitQueue, LogReplay,
    PartitionState, PartitionStateSumComparator};
use super::{FileHeader, FileType, read_head, write_head,
    read_snapshot, write_snapshot, read_log, start_log, write_commit};
use error::{Result};

/// A writable stream for commits
pub enum CommitStream {
    /// This is a new file/object. A header should be written first.
    New(Box<Write>),
    /// This is an append stream on an existing file.
    Append(Box<Write>)
}

/// An interface providing read and/or write access to a suitable location.
pub trait PartitionIO {
    /// Return the number of the latest snapshot found. This may or may not
    /// have commit logs.
    /// 
    /// If this is greater than zero, the snapshot *should* exist, however
    /// if it does not, an attempt should be made to reconstruct it from a
    /// previous snapshot plus commit-logs.
    /// 
    /// For any number less than this and greater than zero, a snapshot may or
    /// may not be present. Commit-logs may or may not be present.
    /// 
    /// Snapshots and commit logs with a number greater than this probably
    /// won't exist and may in any case be ignored.
    /// 
    /// Convention: if this is zero, there may not be an actual snapshot, but
    /// either way the snapshot should be empty (no elements and the state-sum
    /// should be zero).
    /// 
    /// This number must not change except to increase when write_snapshot()
    /// is called.
    fn latest_snapshot_number(&self) -> usize;
    
    /// Get a snapshot with the given number. If no snapshot is present, None
    /// will be returned. If the number is greater than that given by
    /// latest_snapshot_number(), it will return None.
    /// 
    /// This may fail due to IO operations failing.
    fn read_snapshot(&self, ss_num: usize) -> Result<Option<Box<Read>>>;
    
    /// Get the number of log files available for some snapshot
    fn num_logs_for_snapshot(&self, ss_num: usize) -> usize;
    
    /// Get a log file for a snapshot
    fn read_snapshot_log(&self, ss_num: usize, log_num: usize) -> Result<Box<Read>>;
    
    /// Open a write stream on a commit file for the latest snapshot.
    /// 
    /// On success, the write stream is returned along with a boolean which is
    /// true if and only if the write stream is new (has no header).
    /// 
    /// This may be a new file or an existing file to append to.
    fn write_commit(&mut self) -> Result<(Box<Write>, bool)>;
    
    /// Open a write stream on a new snapshot file. This will increase the
    /// number returned by latest_snapshot_number().
    fn write_snapshot(&mut self) -> Result<Box<Write>>;
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
        let latest_num = self.io.latest_snapshot_number();
        let mut num = latest_num;
        
        loop {
            if let Some(mut r) = try!(self.io.read_snapshot(num)) {
                let state = try!(read_snapshot(&mut r));
                self.tips.insert(state.statesum());
                self.states.insert(state);
                break;  // we stop at the most recent snapshot we find
            }
            if num == 0 { break;        /* no more to try; assume zero is empty state */ }
            num -= 1;
        }
        
        let mut queue = CommitQueue::new();
        for ss in num..(latest_num+1) {
            for c in 0..self.io.num_logs_for_snapshot(ss) {
                let mut r = try!(self.io.read_snapshot_log(ss, c));
                let header = try!(read_head(&mut r));
                //TODO: validate header (repo name, partition)
                try!(read_log(&mut r, &mut queue));
            }
        }
        
        let mut replayer = LogReplay::from_sets(&mut self.states, &mut self.tips);
        try!(replayer.replay(queue));
        
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
            let (mut w,is_new) = try!(self.io.write_commit());
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
    pub fn write_snapshot(&mut self) -> Result<()> {
        match self.states.get(&self.parent) {
            None => { Ok(()) },
            Some(state) => {
                let mut writer = try!(self.io.write_snapshot());
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
