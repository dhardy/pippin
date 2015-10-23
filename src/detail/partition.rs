//! Pippin: partition

use std::io::{Read, Write};
use hashindexed::HashIndexed;

use super::{Sum, Commit, PartitionState, PartitionStateSumComparator};
use super::{write_head, read_snapshot, write_snapshot, write_commit};
use error::{Result};

/// A writable stream for commits
pub enum CommitStream {
    /// This is a new file/object. A header should be written first.
    New(Write),
    /// This is an append stream on an existing file.
    Append(Write)
}

/// An interface providing read and/or write access to a suitable location.
pub trait PartitionIO {
    fn read_latest_snapshot(&mut self) -> Result<Box<Read>>;
    
    //TODO: get commit logs on latest snapshot
    //TODO: get all snapshots
    //TODO: get other commit logs
    
    /// Open a write stream on a commit file.
    /// This may be a new file or an existing file to append to.
    fn write_commit(&mut self) -> Result<Box<CommitStream>>;
    
    /// Open a write stream on a new snapshot file.
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
/// How data should be loaded
#[derive(Eq, PartialEq, Debug)]
pub enum LoadMode {
    /// Load only the latest state
    Latest
}

// Methods creating a partition
impl Partition {
    /// Create a new, empty partition. Note that repository partitioning is
    /// automatic, so the only reason you would want to do this would be to
    /// create a new single-partition "repository".
    /// 
    /// `io` must be passed in order to support saving to disk.
    pub fn new(io: Box<PartitionIO>) -> Partition {
        Partition {
            io: io,
            states: HashIndexed::new(),
            unsaved: Vec::new(),
            parent: Sum::zero(),
            cur: PartitionState::new(),
        }
    }
    
    /// Load a partition from snapshot and log files.
    /// 
    /// This may be useful for data-retrieval or single-partition
    /// "repositories" but normally you'd use the repository interface instead.
    /// 
    /// The argument `io` provides access to partition data sources (files),
    /// and the `mode` argument controls which data should be loaded.
    pub fn load(io_: Box<PartitionIO>, mode: LoadMode) -> Result<Partition> {
        assert_eq!( mode, LoadMode::Latest );   // TODO: other modes
        
        let mut io = io_;
        let mut snap_r = try!(io.read_latest_snapshot());
        let state = try!(read_snapshot(&mut snap_r));
        
        let mut states = HashIndexed::new();
        states.insert(state.clone());
        
        //TODO: read commits and apply
        
        Ok(Partition {
            io: io,
            states: states,
            unsaved: Vec::new(),
            parent: state.statesum(),
            cur: state,
        })
    }
}

// Methods saving a partition's data
impl Partition {
    /// Commit changes to the log in memory and optionally on the disk.
    /// 
    /// In all modes, this will create a new commit in memory (if there are any
    /// changes to commit).
    /// 
    /// If mode is at least `CommitMode::FAST_WRITE`, then any pending changes
    /// will be written to the disk.
    /// 
    /// Finally, if mode is `CommitMode::WRITE`, any maintenance operations
    /// required will be done (for example creating a new snapshot when the
    /// current commit log is too long).
    pub fn commit(&mut self, mode: CommitMode) {
        // First step: make a new commit
        // TODO: diff-based commit?
        let last_state = self.states.get(self.parent).unwrap();        // TODO: error
        if self.cur != last_state {
            let state = self.cur.clone();
            self.states.insert(state);
            let commit = Commit::from_diff(last_state, state);
            self.unsaved.push(commit);
        }
        if mode == CommitMode::IN_MEM { return; }
        
        // Second step: write commits
        if !self.unsaved.is_empty() {
            let writer = match try!(self.io.write_commit()) {
                CommitStream::New(w) => {
                    write_head(header, w);      //TODO: commit log header not snapshot!
                    w
                },
                CommitStream::Append(w) => w
            };
            for commit in self.unsaved {
                try!(write_commit(commit, &mut writer));
            }
            self.unsaved.clear();
        }
        if mode == CommitMode::FAST_WRITE { return; }
        
        // Third step: maintenance operations
        if new_snapshot_needed {
            self.write_snapshot();
        }
        assert_eq!( mode, CommitMode::WRITE );
    }
    
    /// Unconditionally write a new snapshot from the latest commit.
    pub fn write_snapshot(&mut self) {
        // TODO: error handling
        let state = self.states.get(self.parent).unwrap();
        let writer = try!(self.io.new_snapshot());
        try!(write_snapshot(state, &mut writer));
    }
}

enum CommitMode {
    // Create a commit but don't save it to the disk
    IN_MEM,
    // Create and write, but don't do any long maintenance operations
    FAST_WRITE,
    // Create, write and do any required maintenance operations
    WRITE
}
