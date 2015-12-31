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
use error::{Result, ArgError, TipError, MatchError, OtherError, make_io_err};

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

/// Determines when to write a new snapshot automatically.
struct SnapshotPolicy {
    counter: usize,
}
impl SnapshotPolicy {
    /// Create a new instance. Assume we have a fresh snapshot.
    fn new() -> SnapshotPolicy { SnapshotPolicy { counter: 0 } }
    /// Report that we definitely need a new snapshot
    fn require(&mut self) { self.counter = 1000; }
    /// Report `n_commits` commits since last event.
    fn add_commits(&mut self, n_commits: usize) { self.counter += n_commits; }
    /// Report that we have a fresh snapshot
    fn reset(&mut self) { self.counter = 0; }
    /// Return true when we should write a snapshot
    fn snapshot(&self) -> bool { self.counter > 100 }
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
    // Determines when to write new snapshots
    ss_policy: SnapshotPolicy,
    // Known committed states indexed by statesum 
    states: HashIndexed<PartitionState, Sum, PartitionStateSumComparator>,
    // All states without a known successor
    tips: HashSet<Sum>,
    // Commits created but not yet saved to disk. First in at front; use as queue.
    unsaved: VecDeque<Commit>,
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
            let header = FileHeader {
                ftype: FileType::Snapshot,
                name: name.to_string(),
                remarks: Vec::new(),
                user_fields: Vec::new(),
            };
            if let Some(mut writer) = try!(io.new_ss(0)) {
                try!(write_head(&header, &mut writer));
                try!(write_snapshot(&state, &mut writer));
            } else {
                return make_io_err(ErrorKind::AlreadyExists, "snapshot already exists");
            }
        }
        
        let mut part = Partition {
            io: io,
            ss_num: 0,
            ss_policy: SnapshotPolicy::new(),
            states: HashIndexed::new(),
            tips: HashSet::new(),
            unsaved: VecDeque::new(),
        };
        part.tips.insert(state.statesum().clone());
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
            ss_policy: SnapshotPolicy::new(),
            states: HashIndexed::new(),
            tips: HashSet::new(),
            unsaved: VecDeque::new(),
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
    pub fn load(&mut self, all_history: bool) -> Result<()> {
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
                    self.tips.insert(state.statesum().clone());
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
                    self.tips.insert(state.statesum().clone());
                    self.states.insert(state);
                    break;  // we stop at the most recent snapshot we find
                }
                if num == 0 {
                    // no more snapshot numbers to try; assume zero is empty state
                    let state = PartitionState::new();
                    self.tips.insert(state.statesum().clone());
                    self.states.insert(state);
                    break;
                }
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
        if num < ss_len -1 {
            self.ss_policy.require();
        } else {
            self.ss_policy.add_commits(num_commits);
        }
        
        if self.tips.is_empty() {
            make_io_err(ErrorKind::NotFound, "load operation found no states")
        } else {
            // success, but a merge may still be required
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
        // #0016: I guess we need to assign repository and partition UUIDs or
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

// Methods saving a partition's data
impl Partition {
    /// Get the state-sum (key) of the tip. Fails when `tip()` fails.
    pub fn tip_key(&self) -> result::Result<&Sum, TipError> {
        if self.tips.len() == 1 {
            Ok(self.tips.iter().next().unwrap())
        } else if self.tips.is_empty() {
            Err(TipError::NotReady)
        } else {
            Err(TipError::MergeRequired)
        }
    }
    
    /// Get a reference to the PartitionState of the current tip. You can read
    /// this directly or make a clone in order to make your modifications.
    /// 
    /// This operation will fail if no data has been loaded yet or a merge is
    /// required.
    /// 
    /// The operation requires some copying but uses copy'c,d-on-write elements
    /// internally. This copy is needed to create a commit from the diff of the
    /// last committed state and the new state.
    pub fn tip(&self) -> result::Result<&PartitionState, TipError> {
        Ok(&self.states.get(try!(self.tip_key())).unwrap())
    }
    
    /// Get a read-only reference to a state by its statesum, if found.
    /// 
    /// If you want to keep a copy, clone it.
    pub fn state(&self, key: &Sum) -> Option<&PartitionState> {
        self.states.get(key)
    }
    
    /// Try to find a state given a string representation of the key (as a byte array).
    /// 
    /// Like git, we accept partial keys (so long as they uniquely resolve a key).
    pub fn state_from_string(&self, string: String) -> Result<&PartitionState, MatchError> {
        let string = string.to_uppercase().replace(" ", "");
        let mut matching = Vec::new();
        for state in self.states.iter() {
            if state.statesum().matches_string(&string.as_bytes()) {
                matching.push(state.statesum());
            }
            if matching.len() > 1 {
                return Err(MatchError::MultiMatch(
                    matching[0].as_string(false), matching[1].as_string(false)));
            }
        }
        if matching.len() == 1 {
            Ok(self.states.get(&matching[0]).unwrap())
        } else {
            Err(MatchError::NoMatch)
        }
    }
    
    // #0003: allow getting a reference to other states listing snapshots, commits, getting non-current states and
    // getting diffs.
    
    /// This will create a new commit in memory by comparing the passed state
    /// to its parent, held internally. If there are no changes to the passed
    /// state, nothing happens.
    /// 
    /// A merge might be required after calling this (if the parent state of
    /// that passed is no longer a 'tip').
    /// 
    /// Use `write()` afterwards to save newly created commits to an external
    /// resource (e.g. a file).
    /// 
    /// Returns true if a new commit was made, false if no changes were found.
    pub fn commit(&mut self, state: PartitionState) -> Result<bool> {
        // #0019: Commit::from_diff compares old and new states and code be slow.
        // #0019: Instead, we could record each alteration as it happens.
        let c = if state.statesum() == state.parent() {
            None
        } else {
            let parent = try!(self.states.get(state.parent())
                    .ok_or(ArgError::new("parent state not found")));
            Commit::from_diff(parent, &state)
        };
        if let Some(commit) = c {
            self.unsaved.push_back(commit);
            self.tips.remove(state.parent());   // this might fail (if the parent was not a tip)
            self.tips.insert(state.statesum().clone());
            self.states.insert(state);
            Ok(true)
        } else {
            Ok(false)
        }
    }
    
    /// This will write all unsaved commits to a log on the disk.
    /// 
    /// If `fast` is true, no further actions will happen, otherwise required
    /// maintenance operations will be carried out (e.g. creating a new
    /// snapshot when the current commit-log is long).
    /// 
    /// Returns true if any commits were written (i.e. unsaved commits
    /// were found). Returns false if nothing needed doing.
    /// 
    /// Note that writing to disk can fail. In this case it may be worth trying
    /// again.
    pub fn write(&mut self, fast: bool) -> Result<bool> {
        // First step: write commits
        let has_changes = !self.unsaved.is_empty();
        if has_changes {
            // #0012: extend existing logs instead of always writing a new log file.
            let mut cl_num = self.io.ss_cl_len(self.ss_num);
            loop {
                if let Some(mut writer) = try!(self.io.new_ss_cl(self.ss_num, cl_num)) {
                    // Write a header since this is a new file:
                    let header = FileHeader {
                        ftype: FileType::CommitLog,
                        name: "".to_string() /* #0016: repo name?*/,
                        remarks: Vec::new(),
                        user_fields: Vec::new(),
                    };
                    try!(write_head(&header, &mut writer));
                    try!(start_log(&mut writer));
                    
                    // Now write commits:
                    while !self.unsaved.is_empty() {
                        // We try to write the commit, then when successful remove it
                        // from the list of 'unsaved' commits.
                        try!(write_commit(&self.unsaved.front().unwrap(), &mut writer));
                        self.unsaved.pop_front().expect("pop_front");
                        self.ss_policy.add_commits(1);
                    }
                    break;
                } else {
                    // Log file already exists! So try another number.
                    if cl_num > 1000_000 {
                        // We should give up eventually. When is arbitrary.
                        return Err(box OtherError::new("Commit log number too high"));
                    }
                    cl_num += 1;
                }
            }
        }
        
        // Second step: maintenance operations
        if !fast {
            if self.is_ready() && self.ss_policy.snapshot() {
                try!(self.write_snapshot());
            }
        }
        
        Ok(has_changes)
    }
    
    /// Write a new snapshot from the tip.
    /// 
    /// Normally you can just call `write()` and let the library figure out
    /// when to write a new snapshot, though you can also call this directly.
    /// 
    /// Fails when `tip()` fails.
    pub fn write_snapshot(&mut self) -> Result<()> {
        // fail early if not ready:
        let tip_key = try!(self.tip_key()).clone();
        
        let mut ss_num = self.ss_num + 1;
        loop {
            // Try to get a writer for this snapshot number:
            if let Some(mut writer) = try!(self.io.new_ss(ss_num)) {
                let header = FileHeader {
                    ftype: FileType::Snapshot,
                    name: "".to_string() /* #0016: repo name?*/,
                    remarks: Vec::new(),
                    user_fields: Vec::new(),
                };
                try!(write_head(&header, &mut writer));
                try!(write_snapshot(self.states.get(&tip_key).unwrap(), &mut writer));
                self.ss_num = ss_num;
                self.ss_policy.reset();
                return Ok(())
            } else {
                // Snapshot file already exists! So try another number.
                if ss_num > 1000_000 {
                    // We should give up eventually. When is arbitrary.
                    return Err(box OtherError::new("Snapshot number too high"));
                }
                ss_num += 1;
            }
        }
    }
}


#[test]
fn on_new_partition() {
    use super::Element;
    
    let io = box PartitionDummyIO::new();
    let mut part = Partition::new(io, "on_new_partition").expect("partition creation");
    assert_eq!(part.tips.len(), 1);
    
    let state = part.tip().expect("getting tip").clone_child();
    assert_eq!(state.parent(), &Sum::zero());
    assert_eq!(part.commit(state).expect("committing"), false);
    
    let mut state = part.tip().expect("getting tip").clone_child();
    assert!(state.is_empty());
    assert_eq!(state.statesum(), &Sum::zero());
    
    let elt1 = Element::from_str("This is element one.");
    let elt2 = Element::from_str("Element two data.");
    let mut key = elt1.sum().clone();
    key.permute(elt2.sum());
    assert!(state.insert_elt(1, elt1).is_ok());
    assert!(state.insert_elt(2, elt2).is_ok());
    assert_eq!(state.statesum(), &key);
    
    assert_eq!(part.commit(state).expect("comitting"), true);
    assert_eq!(part.unsaved.len(), 1);
    assert_eq!(part.states.len(), 2);
    {
        let state = part.state(&key).expect("getting state by key");
        assert!(state.has_elt(1));
        assert_eq!(state.get_elt(2), Some(&Element::from_str("Element two data.")));
    }   // `state` goes out of scope
    assert_eq!(part.tips.len(), 1);
    let state = part.tip().expect("getting tip").clone_child();
    assert_eq!(state.parent(), &key);
    
    assert_eq!(part.commit(state).expect("committing"), false);
}
