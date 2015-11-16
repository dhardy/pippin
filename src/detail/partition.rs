//! Pippin: partition

use std::io::{Read, Write, ErrorKind};
use std::collections::HashSet;
use std::result;
use hashindexed::HashIndexed;

use super::{Sum, Commit, CommitQueue, LogReplay,
    PartitionState, PartitionStateSumComparator};
use super::{FileHeader, FileType, read_head, write_head,
    read_snapshot, write_snapshot, read_log, start_log, write_commit};
use error::{Result, Error};

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
    
    // TODO: other operations (delete file, ...?)
}

/// Doesn't provide any IO.
/// 
/// Can be used for testing but big fat warning: this does not provide any
/// method to save your data. Write operations fail with `ErrorKind::InvalidInput`.
pub struct PartitionDummyIO;

impl PartitionIO for PartitionDummyIO {
    fn ss_len(&self) -> usize { 0 }
    fn ss_cl_len(&self, _ss_num: usize) -> usize { 0 }
    fn read_ss(&self, _ss_num: usize) -> Result<Option<Box<Read>>> {
        Ok(None)
    }
    fn read_ss_cl(&self, _ss_num: usize, _cl_num: usize) -> Result<Option<Box<Read>>> {
        Ok(None)
    }
    fn new_ss(&mut self, _ss_num: usize) -> Result<Box<Write>> {
        Err(Error::io(ErrorKind::InvalidInput, "PartitionDummyIO does not support writing"))
    }
    fn append_ss_cl(&mut self, _ss_num: usize, _cl_num: usize) -> Result<Box<Write>> {
        Err(Error::io(ErrorKind::InvalidInput, "PartitionDummyIO does not support writing"))
    }
    fn new_ss_cl(&mut self, _ss_num: usize, _cl_num: usize) -> Result<Box<Write>> {
        Err(Error::io(ErrorKind::InvalidInput, "PartitionDummyIO does not support writing"))
    }
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
    // Current state, to be modified. This is stored internally so that we do
    // not have to trust the user, since if (a) the wrong state is committed, 
    // it will replace everything in the partition (except history), and (b) if
    // a state is held externally while the `Partition` is updated with new
    // commits from another source, then that externally held state is used to
    // make a new commit, the changes from the other commits will be reverted.
    // It is only usable if tips.len() == 1
    current: PartitionState,
}

// Methods creating a partition and loading its data
impl Partition {
    /// Create a partition, assigning an IO provider (this can only be done at
    /// time of creation).
    /// 
    /// The partition will not be *ready* until either declared `new()` or data
    /// is loaded with one of the load operations. Until it is *ready* most
    /// operations will fail.
    /// 
    /// Example:
    /// 
    /// ```no_run
    /// use std::path::Path;
    /// use pippin::{Partition, DiscoverPartitionFiles};
    /// 
    /// let path = Path::new(".");
    /// let io = DiscoverPartitionFiles::from_dir_basename(path, "my-partition").unwrap();
    /// let partition = Partition::create(Box::new(io));
    /// ```
    pub fn create(io: Box<PartitionIO>) -> Partition {
        Partition {
            io: io,
            states: HashIndexed::new(),
            tips: HashSet::new(),
            unsaved: Vec::new(),
            parent: Sum::zero() /* key to initial state */,
            current: PartitionState::new() /* copy of initial state */,
        }
    }
    
    /// Declare that this is a new partition with no prior history and mark it
    /// *ready*.
    /// 
    /// This inserts an initial, empty, state.
    /// 
    /// Note that if you do this on a `Partition` that is not freshly created
    /// all data will be lost, hence why this method consumes and regurgitates
    /// the `Partition`.
    /// 
    /// Example:
    /// 
    /// ```
    /// use pippin::{Partition, PartitionDummyIO};
    /// 
    /// let partition = Partition::create(Box::new(PartitionDummyIO)).new();
    /// ```
    pub fn new(mut self) -> Partition {
        let state = PartitionState::new();
        self.current = state.clone();
        self.parent = state.statesum();
        self.tips.clear();
        self.tips.insert(state.statesum());
        self.states.clear();    // free memory if not empty
        self.states.insert(state);
        self.unsaved.clear();
        self
    }
    
    /// Load the latest known state of a partition, using the latest available
    /// snapshot and all subsequent commit logs.
    /// 
    /// If the partition contains data before the load, any changes will be
    /// committed (in-memory only) and newly loaded data will be seamlessly
    /// merged with that already loaded. A merge may be required; it is also
    /// possible that tips may be part of disconnected graphs and thus
    /// unmergable as with `load_everything()`.
    /// 
    /// After the operation, the repository may be in one of three states: no
    /// known states, one directed graph of one or more states with a single
    /// tip (latest state), or a graph with multiple tips (requiring a merge
    /// operation).
    /// 
    /// TODO: fail if no data is found? Require call to merge_required()?
    /// 
    /// Calls `self.commit()` internally to save any modifications as a new commit.
    pub fn load_latest(&mut self) -> Result<()> {
        try!(self.commit());
        
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
        
        {
            let mut replayer = LogReplay::from_sets(&mut self.states, &mut self.tips);
            try!(replayer.replay(queue));
        }// end borrow of self.tips
        
        if self.tips.is_empty() {
            Err(Error::io(ErrorKind::NotFound, "load operation found no states"))
        } else {
            // success, but a merge may still be required
            if self.tips.len() == 1 {
                let tip = self.tips.iter().next().expect("len is 1 so next() should yield an element");
                self.current = self.states.get(tip).expect("state for tip should be present").clone();
            }
            Ok(())
        }
    }
    
    /// Load all history available.
    /// 
    /// This operation is similar to `load_latest()`, with the following
    /// differences:
    /// 
    /// 1.  All snapshots and commits found are loaded, not just the latest.
    /// 2.  When multiple tips are present after the operation, it is possible
    ///     for tips to be unconnected (i.e. there to be more than one directed
    ///     graph). In this case it may not be possible to merge all tips into
    ///     a single latest state.
    pub fn load_everything(&mut self) -> Result<()> {
        try!(self.commit());
        
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
        
        if self.tips.is_empty() {
            Err(Error::io(ErrorKind::NotFound, "load operation found no states"))
        } else {
            // success, but a merge may still be required
            if self.tips.len() == 1 {
                let tip = self.tips.iter().next().expect("len is 1 so next() should yield an element");
                self.current = self.states.get(tip).expect("state for tip should be present").clone();
            }
            Ok(())
        }
    }
    
    /// Returns true while a merge is required.
    /// 
    /// Returns false if not ready or no tip is found as well as when a single
    /// tip is present and ready to use.
    pub fn merge_required(&self) -> bool {
        self.tips.len() > 1
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

#[derive(PartialEq, Eq, Debug)]
pub enum TipError {
    /// Partition has not yet been loaded or set "new".
    NotReady,
    /// Loaded data left multiple tips. A merge is required to create a single
    /// tip.
    MergeRequired,
}

// Methods saving a partition's data
impl Partition {
    /// Get a reference to the PartitionState of the current tip. You can read
    /// this directly or make a clone in order to make your modifications.
    /// 
    /// This operation will fail if no data has been loaded yet or a merge is
    /// required.
    /// 
    /// The operation requires some copying but uses copy-on-write elements
    /// internally. This copy is needed to create a commit from the diff of the
    /// last committed state and the new state.
    pub fn tip(&mut self) -> result::Result<&mut PartitionState, TipError> {
        if self.tips.len() == 1 {
            Ok(&mut self.current)
        } else if self.tips.is_empty() {
            Err(TipError::NotReady)
        } else {
            Err(TipError::MergeRequired)
        }
    }
    
    /// Get a read-only reference to a state by its statesum, if found.
    pub fn get(&self, key: &Sum) -> Option<&PartitionState> {
        self.states.get(key)
    }
    
    // TODO: allow getting a reference to other states listing snapshots, commits, getting non-current states and
    // getting diffs.
    
    /// This will create a new commit in memory (if there are any changes to
    /// commit). Use `write()` if you want to save commits to an external
    /// resource (file or whatever).
    /// 
    /// Returns true if a new commit was made, false if no changes were found.
    pub fn commit(&mut self) -> Result<bool> {
        // TODO: diff-based commit?
        let c = if self.parent == Sum::zero() {
            if self.current.is_empty() {
                None
            } else {
                Some(Commit::from_diff(&PartitionState::new(), &self.current))
            }
        } else {
            let state = self.states.get(&self.parent).expect("parent state not found");
            if self.current == *state {
                None
            } else {
                // We cannot modify self.states here due to borrow, hence
                // return value and next 'if' block.
                Some(Commit::from_diff(state, &self.current))
            }
        };
        if let Some(commit) = c {
            self.unsaved.push(commit);
            self.states.insert(self.current.clone());
            self.tips.remove(&self.parent);
            self.parent = self.current.statesum();
            self.tips.insert(self.parent);
            Ok(true)
        } else {
            Ok(false)
        }
    }
    
    //TODO: revise (remove "mode"?)
    /// Commit changes to the log in memory and optionally on the disk.
    /// 
    /// This will create a new commit in memory (if there are any changes to
    /// commit). It will then write all unsaved commits to a log on the disk.
    /// 
    /// If `fast` is true, no further actions will happen, otherwise required
    /// maintenance operations will be carried out (e.g. creating a new
    /// snapshot when the current commit-log is long).
    /// 
    /// Returns true if any new commits were made (i.e. changes were pending)
    /// or if commits were pending writing. Returns false if nothing needed
    /// doing.
    /// 
    /// Note that writing to disk can fail. In this case it may be worth trying
    /// again
    pub fn write(&mut self, fast: bool) -> Result<bool> {
        // First step: make a new commit
        try!(self.commit());
        
        // Second step: write commits
        let result = if self.unsaved.is_empty() {
            false
        } else {
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
            true
        };
        if fast { return Ok(result); }
        
        // Third step: maintenance operations
        if false /*TODO new_snapshot_needed*/ {
            try!(self.write_snapshot());
        }
        Ok(result)
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


#[test]
fn on_new_partition() {
    use super::Element;
    
    let io = box PartitionDummyIO;
    let mut part = Partition::create(io).new();
    assert_eq!(part.tips.len(), 1);
    assert_eq!(part.parent, Sum::zero());
    
    assert_eq!(part.commit().expect("is okay"), false);
    
    let key = {
        let state = part.tip().expect("tip is ready");
        assert!(state.is_empty());
        assert_eq!(state.statesum(), Sum::zero());
        
        let elt1 = Element::from_str("This is element one.");
        let elt2 = Element::from_str("Element two data.");
        let mut key = elt1.sum().clone();
        key.permute(elt2.sum());
        assert!(state.insert_elt(1, elt1).is_ok());
        assert!(state.insert_elt(2, elt2).is_ok());
        assert_eq!(state.statesum(), key);
        key
    };   // `state` goes out of scope
    
    assert_eq!(part.commit().expect("is okay"), true);
    assert_eq!(part.unsaved.len(), 1);
    assert_eq!(part.states.len(), 2);
    {
        let state = part.get(&key).expect("state should exist");
        assert!(state.has_elt(1));
        assert_eq!(state.get_elt(2), Some(&Element::from_str("Element two data.")));
    }   // `state` goes out of scope
    assert_eq!(part.parent, key);
    assert_eq!(part.tips.len(), 1);
    
    assert_eq!(part.commit().expect("is okay"), false);
}

//TODO: test IO, loading of an existing partition, reading of historical states
//TODO: and merge operations.
