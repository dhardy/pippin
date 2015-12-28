//! Pippin: partition

use std::io::{Read, Write, ErrorKind};
use std::collections::{HashSet, VecDeque};
use std::result;
use std::any::Any;
use hashindexed::HashIndexed;

use super::{Sum, Commit, CommitQueue, LogReplay,
    PartitionState, PartitionStateSumComparator};
use super::readwrite::{FileHeader, FileType, read_head, write_head,
    read_snapshot, write_snapshot, read_log, start_log, write_commit};
use error::{Result, Error, make_io_err};

/// An interface providing read and/or write access to a suitable location.
/// 
/// Note: lifetimes on some functions are more restrictive than might seem
/// necessary; this is to allow an implementation which reads and writes to
/// internal streams.
pub trait PartitionIO {
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
    
    /// Get a snapshot with the given number. If no snapshot is present or if
    /// ss_num is too large, None will be returned.
    /// 
    /// This can fail due to IO operations failing.
    fn read_ss<'a>(&'a self, ss_num: usize) -> Result<Option<Box<Read+'a>>>;
    
    /// Get a commit log (numbered `cl_num`) file for a snapshot (numbered
    /// `ss_num`). If none is found, return Ok(None).
    fn read_ss_cl<'a>(&'a self, ss_num: usize, cl_num: usize) -> Result<Option<Box<Read+'a>>>;
    
    /// Open a write stream on a new snapshot file, numbered ss_num.
    /// This will increase the number returned by ss_len().
    /// 
    /// Fails if a snapshot with number ss_num already exists.
    fn new_ss<'a>(&'a mut self, ss_num: usize) -> Result<Box<Write+'a>>;
    
    /// Open an append-write stream on an existing commit file. Writes may be
    /// atomic. Each commit should be written via a single write operation.
    //TODO: verify atomicity of writes
    fn append_ss_cl<'a>(&'a mut self, ss_num: usize, cl_num: usize) -> Result<Box<Write+'a>>;
    
    /// Open a write-stream on a new commit file. As with the append version,
    /// the file will be opened in append mode, thus writes may be atomic.
    /// Each commit (and the header, including commit section marker) should be
    /// written via a single write operation.
    /// 
    /// Fails if a commit log with number `cl_num` for snapshot `ss_num`
    /// already exists.
    //TODO: verify atomicity of writes
    fn new_ss_cl<'a>(&'a mut self, ss_num: usize, cl_num: usize) -> Result<Box<Write+'a>>;
    
    // TODO: other operations (delete file, ...?)
}

/// Doesn't provide any IO.
/// 
/// Can be used for testing but big fat warning: this does not provide any
/// method to save your data. Write operations fail with `ErrorKind::InvalidInput`.
pub struct PartitionDummyIO {
    // The internal buffer allows us to accept write operations. Data gets
    // written over on the next write.
    buf: Vec<u8>
}
impl PartitionDummyIO {
    /// Create a new instance
    pub fn new() -> PartitionDummyIO {
        PartitionDummyIO { buf: Vec::new() }
    }
}

impl PartitionIO for PartitionDummyIO {
    fn as_any(&self) -> &Any { self }
    fn ss_len(&self) -> usize { 0 }
    fn ss_cl_len(&self, _ss_num: usize) -> usize { 0 }
    fn read_ss(&self, _ss_num: usize) -> Result<Option<Box<Read+'static>>> {
        Ok(None)
    }
    fn read_ss_cl(&self, _ss_num: usize, _cl_num: usize) -> Result<Option<Box<Read+'static>>> {
        Ok(None)
    }
    fn new_ss<'a>(&'a mut self, _ss_num: usize) -> Result<Box<Write+'a>> {
        self.buf.clear();
        Ok(Box::new(&mut self.buf))
    }
    fn append_ss_cl<'a>(&'a mut self, _ss_num: usize, _cl_num: usize) -> Result<Box<Write+'a>> {
        self.buf.clear();
        Ok(Box::new(&mut self.buf))
    }
    fn new_ss_cl<'a>(&'a mut self, _ss_num: usize, _cl_num: usize) -> Result<Box<Write+'a>> {
        self.buf.clear();
        Ok(Box::new(&mut self.buf))
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
/// 
/// A partition is in one of three possible states: (1) unloaded, (2) loaded
/// but requiring a merge (multiple tips), (3) ready for use.
pub struct Partition {
    // IO provider
    io: Box<PartitionIO>,
    // Number of the current snapshot file
    ss_num: usize,
    // Set to true if a new snapshot is required before further commit logs are
    // written (e.g. file corruption, missing snapshot, blank state).
    need_snapshot: bool,
    // Known committed states indexed by statesum 
    states: HashIndexed<PartitionState, Sum, PartitionStateSumComparator>,
    // All states without a known successor
    tips: HashSet<Sum>,
    // Commits created but not yet saved to disk. First in at front; use as queue.
    unsaved: VecDeque<Commit>,
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
    //TODO: neither of these limitations hold if we merge changes. Remove.
    current: PartitionState,
}

// Methods creating a partition and loading its data
impl Partition {
    /// Create a partition, assigning an IO provider (this can only be done at
    /// time of creation). Create a blank state in the partition, write an
    /// empty snapshot to the provided `PartitionIO`, and mark self as *ready
    /// for use*.
    /// 
    /// Example:
    /// 
    /// ```
    /// use pippin::{Partition, PartitionDummyIO};
    /// 
    /// let partition = Partition::new(Box::new(PartitionDummyIO::new()), "example repo");
    /// ```
    pub fn new(mut io: Box<PartitionIO>, name: &str) -> Result<Partition> {
        let state = PartitionState::new();
        {
            let mut writer = try!(io.new_ss(0));
            let header = FileHeader {
                ftype: FileType::Snapshot,
                name: name.to_string(),
                remarks: Vec::new(),
                user_fields: Vec::new(),
            };
            try!(write_head(&header, &mut writer));
            try!(write_snapshot(&state, &mut writer));
        }
        
        let mut part = Partition {
            io: io,
            ss_num: 0,
            need_snapshot: false,
            states: HashIndexed::new(),
            tips: HashSet::new(),
            unsaved: VecDeque::new(),
            parent: state.statesum(),
            current: state.clone(),
        };
        part.tips.insert(state.statesum());
        part.states.insert(state);
        
        Ok(part)
    }
    
    /// Create a partition, assigning an IO provider (this can only be done at
    /// time of creation).
    /// 
    /// The partition will not be *ready to use* until data is loaded with one
    /// of the load operations. Until then most operations will fail.
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
            ss_num: 0,
            need_snapshot: false,
            states: HashIndexed::new(),
            tips: HashSet::new(),
            unsaved: VecDeque::new(),
            parent: Sum::zero() /* key to initial state */,
            current: PartitionState::new() /* copy of initial state */,
        }
    }
    
    /// Load either all history available or only that required to find the
    /// latest state of the partition. Uses snapshot and log files provided by
    /// the provided `PartitionIO`.
    /// 
    /// If `all_history == true`, all snapshots and commits found are loaded.
    /// In this case it is possible that the history graph is not connected
    /// (i.e. it has multiple unconnected sub-graphs). If this is the case,
    /// the usual merge strategy will fail.
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
    /// TODO: don't fail, just report warnings?
    /// 
    /// Calls `self.commit()` internally to save any modifications as a new commit.
    pub fn load(&mut self, all_history: bool) -> Result<()> {
        try!(self.commit());
        
        let ss_len = self.io.ss_len();
        let mut num = ss_len - 1;
        let mut num_commits = 0;
        
        if all_history {
            for ss in 0..ss_len {
                let result = if let Some(mut r) = try!(self.io.read_ss(ss)) {
                    let head = try!(read_head(&mut r));
                    let snapshot = try!(read_snapshot(&mut r));
                    Some((head, snapshot))
                } else { None };
                if let Some((head, state)) = result {
                    try!(self.validate_header(head));
                    self.tips.insert(state.statesum());
                    self.states.insert(state);
                }
                
                let mut queue = CommitQueue::new();
                for c in 0..self.io.ss_cl_len(ss) {
                    let result = if let Some(mut r) = try!(self.io.read_ss_cl(ss, c)) {
                        let head = try!(read_head(&mut r));
                        try!(read_log(&mut r, &mut queue));
                        Some(head)
                    } else { None };
                    if let Some(head) = result {
                        // Note: if this error becomes recoverable within load_latest(),
                        // the header should be validated before updating `queue`.
                        try!(self.validate_header(head));
                    }
                }
                num_commits = queue.len();  // final value is number of commits after last snapshot
                let mut replayer = LogReplay::from_sets(&mut self.states, &mut self.tips);
                try!(replayer.replay(queue));
            }
        } else {
            loop {
                let result = if let Some(mut r) = try!(self.io.read_ss(num)) {
                    let head = try!(read_head(&mut r));
                    let snapshot = try!(read_snapshot(&mut r));
                    Some((head, snapshot))
                } else { None };
                if let Some((head, state)) = result {
                    try!(self.validate_header(head));
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
                    let result = if let Some(mut r) = try!(self.io.read_ss_cl(ss, c)) {
                        let head = try!(read_head(&mut r));
                        try!(read_log(&mut r, &mut queue));
                        Some(head)
                    } else { None };
                    if let Some(head) = result {
                        // Note: if this error becomes recoverable within load_latest(),
                        // the header should be validated before updating `queue`.
                        try!(self.validate_header(head));
                    }
                }
            }
            num_commits = queue.len();
            let mut replayer = LogReplay::from_sets(&mut self.states, &mut self.tips);
            try!(replayer.replay(queue));
        }
        
        self.ss_num = ss_len - 1;
        // We should make a new snapshot if the last one is missing or we have
        // many commits (in total). TODO: proper new-snapshot policy.
        self.need_snapshot = num < ss_len - 1 || num_commits > 100;
        
        if self.tips.is_empty() {
            make_io_err(ErrorKind::NotFound, "load operation found no states")
        } else {
            // success, but a merge may still be required
            if self.tips.len() == 1 {
                let tip = self.tips.iter().next().expect("len is 1 so next() should yield an element");
                self.current = self.states.get(tip).expect("state for tip should be present").clone();
                self.parent = *tip;
            }
            Ok(())
        }
    }
    
    /// Returns true when elements have been loaded (though also see
    /// `merge_required`).
    pub fn is_loaded(&self) -> bool {
        self.tips.len() > 0
    }
    
    /// Returns true when ready for use (this is equivalent to
    /// `part.is_loaded() && !part.merge_required()`).
    pub fn is_ready(&self) -> bool {
        self.tips.len() == 1
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
    
    /// Unload data from memory. Note that unless `force == true` the operation
    /// will fail if any changes have not yet been saved to disk.
    /// 
    /// Returns true if data was unloaded, false if not (implies `!force` and 
    /// that unsaved changes exist).
    pub fn unload(&mut self, force: bool) -> bool {
        if force || self.unsaved.is_empty() {
            self.states.clear();
            self.tips.clear();
            true
        } else {
            false
        }
    }
    
    /// Consume the `Partition` and return the held `PartitionIO`.
    /// 
    /// This destroys all states held internally, but states may be cloned
    /// before unwrapping. Since `Element`s are copy-on-write, cloning
    /// shouldn't be too expensive.
    pub fn unwrap_io(self) -> Box<PartitionIO> {
        self.io
    }
}

/// Error type returned by `Partition::tip()`.
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
    //TODO: instead of returning a &mut reference to self.current, we should make the user clone.
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
    /// 
    /// If you want to keep a copy, clone it.
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
                Commit::from_diff(&PartitionState::new(), &self.current)
            }
        } else {
            let state = self.states.get(&self.parent).expect("parent state not found");
            if self.current == *state {
                None
            } else {
                // We cannot modify self.states here due to borrow, hence
                // return value and next 'if' block.
                Commit::from_diff(state, &self.current)
            }
        };
        if let Some(commit) = c {
            self.unsaved.push_back(commit);
            self.states.insert(self.current.clone());
            self.tips.remove(&self.parent);
            self.parent = self.current.statesum();
            self.tips.insert(self.parent);
            Ok(true)
        } else {
            Ok(false)
        }
    }
    
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
            // TODO: extend existing logs instead of always writing a new log file.
            
            let cl_num = self.io.ss_cl_len(self.ss_num);     // new commit (TODO don't always want this)
            let (mut w,is_new) = (try!(self.io.new_ss_cl(self.ss_num, cl_num)), true);
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
                try!(write_commit(&self.unsaved.front().unwrap(), &mut w));
                self.unsaved.pop_front().expect("pop_front");
            }
            true
        };
        if fast { return Ok(result); }
        
        // Third step: maintenance operations
        if self.need_snapshot {
            try!(self.write_snapshot());
            self.need_snapshot = false;
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
        if let Some(state) = self.states.get(&self.parent) {
            //TODO: we should try a new snapshot number if this fails with AlreadyExists
            let ss_num = self.ss_num + 1;
            let mut writer = try!(self.io.new_ss(ss_num));
            let header = FileHeader {
                ftype: FileType::Snapshot,
                name: "".to_string() /*TODO: repo name?*/,
                remarks: Vec::new(),
                user_fields: Vec::new(),
            };
            try!(write_head(&header, &mut writer));
            try!(write_snapshot(state, &mut writer));
            self.ss_num = ss_num;
            self.need_snapshot = false;
        }
        Ok(())
    }
}


#[test]
fn on_new_partition() {
    use super::Element;
    
    let io = box PartitionDummyIO::new();
    let mut part = Partition::new(io, "on_new_partition").expect("partition creation");
    assert_eq!(part.tips.len(), 1);
    assert_eq!(part.parent, Sum::zero());
    
    assert_eq!(part.commit().expect("committing"), false);
    
    let key = {
        let state = part.tip().expect("getting tip");
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
    
    assert_eq!(part.commit().expect("comitting"), true);
    assert_eq!(part.unsaved.len(), 1);
    assert_eq!(part.states.len(), 2);
    {
        let state = part.get(&key).expect("getting state by key");
        assert!(state.has_elt(1));
        assert_eq!(state.get_elt(2), Some(&Element::from_str("Element two data.")));
    }   // `state` goes out of scope
    assert_eq!(part.parent, key);
    assert_eq!(part.tips.len(), 1);
    
    assert_eq!(part.commit().expect("committing"), false);
}

//TODO: test IO, loading of an existing partition, reading of historical states
//TODO: and merge operations.
