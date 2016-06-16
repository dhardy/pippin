/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Pippin: partition

use std::io::{Read, Write, ErrorKind};
use std::collections::{HashSet, VecDeque};
use std::result;
use std::any::Any;
use std::ops::Deref;
use std::usize;
use std::cmp::min;
use hashindexed::{HashIndexed, Iter};

pub use detail::states::{State, MutState, PartState, MutPartState};

use detail::readwrite::{FileHeader, UserData, FileType, read_head, write_head, validate_repo_name};
use detail::readwrite::{read_snapshot, write_snapshot};
use detail::readwrite::{read_log, start_log, write_commit};
use detail::states::{PartStateSumComparator};
use commit::{Commit, MakeMeta};
use merge::{TwoWayMerge, TwoWaySolver};
use {ElementT, Sum, PartId};
use error::{Result, TipError, PatchOp, MatchError, MergeError, OtherError, make_io_err};

/// An interface providing read and/or write access to a suitable location.
/// 
/// Note: lifetimes on some functions are more restrictive than might seem
/// necessary; this is to allow an implementation which reads and writes to
/// internal streams.
pub trait PartIO {
    /// Convert self to a `&Any`
    fn as_any(&self) -> &Any;
    
    /// Return the partition identifier.
    fn part_id(&self) -> PartId;
    
    /// Defines our snapshot policy: this should return true when a new
    /// snapshot is required. Parameters: `commits` is the number of commits
    /// since the last snapshot and `edits` is the number of element changes
    /// counted since the last snapshot (the true number of edits may be
    /// slightly higher).
    /// 
    /// In unusual cases, the partition is marked as "definitely needing a
    /// snapshot". This is done by setting commits to a large number
    /// (between 0x10_0000 and 0x100_0000).
    /// 
    /// The default implementation is
    /// ```rust
    /// commits * 5 + edits > 150
    /// ```
    fn want_snapshot(&self, commits: usize, edits: usize) -> bool {
        commits * 5 + edits > 150
    }
    
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

/// Provide access to user fields of header
pub trait UserFields {
    /// Generate user fields to be included in a header. If you don't wish to
    /// add extra fields, just return `vec![]` or `Vec::new()`.
    /// 
    /// `part_id`: the partition identifier. Given so that the same interface
    ///     can be used within repositories.
    /// 
    /// `is_log`: false if this is to be written to a snapshot file, true if
    /// this is to be written to a log file. Users may choose not to write to
    /// both.
    fn write_user_fields(&mut self, part_id: PartId, is_log: bool) -> Vec<UserData>;
    /// Take user fields found in a header and read or discard them.
    /// 
    /// `part_id`: the partition identifier. Given so that the same interface
    ///     can be used within repositories.
    /// 
    /// `is_log`: false if this is read from a snapshot file, true if this is
    /// read from a log file. Users may choose not to write to both.
    fn read_user_fields(&mut self, user: Vec<UserData>, part_id: PartId, is_log: bool);
}

/// Doesn't provide any IO.
/// 
/// Can be used for testing but big fat warning: this does not provide any
/// method to save your data. Write operations fail with `ErrorKind::InvalidInput`.
pub struct DummyPartIO {
    part_id: PartId,
    // The internal buffer allows us to accept write operations. Data gets
    // written over on the next write.
    buf: Vec<u8>
}
impl DummyPartIO {
    /// Create a new instance
    pub fn new(part_id: PartId) -> DummyPartIO {
        DummyPartIO { part_id: part_id, buf: Vec::new() }
    }
}

impl PartIO for DummyPartIO {
    fn as_any(&self) -> &Any { self }
    fn part_id(&self) -> PartId { self.part_id }
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
/// 
/// Terminology: a *tip* (as in *point* or *peak*) is a state without a known
/// successor. Normally there is exactly one tip, but see `is_ready`,
/// `is_loaded` and `merge_required`.
pub struct Partition<E: ElementT> {
    // IO provider
    io: Box<PartIO>,
    // Partition name. Used to identify loaded files.
    repo_name: String,
    // Partition identifier
    part_id: PartId,
    // Number of first snapshot file loaded (equal to ss1 if nothing is loaded)
    ss0: usize,
    // Number of latest snapshot file loaded + 1; 0 if nothing loaded and never less than ss0
    ss1: usize,
    // Determines when to write new snapshots
    ss_commits: usize,
    ss_edits: usize,
    // Known committed states indexed by statesum 
    states: HashIndexed<PartState<E>, Sum, PartStateSumComparator>,
    // All states not in `states` which are known to be superceded
    parents: HashSet<Sum>,
    // All states without a known successor
    tips: HashSet<Sum>,
    // Commits created but not yet saved to disk. First in at front; use as queue.
    unsaved: VecDeque<Commit<E>>,
}

// Methods creating a partition, loading its data or checking status
impl<E: ElementT> Partition<E> {
    /// Create a partition, assigning an IO provider (this can only be done at
    /// time of creation). Create a blank state in the partition, write an
    /// empty snapshot to the provided `PartIO`, and mark self as *ready
    /// for use*.
    /// 
    /// Example:
    /// 
    /// ```
    /// use pippin::{Partition, PartId};
    /// use pippin::part::DummyPartIO;
    /// 
    /// let io = Box::new(DummyPartIO::new(PartId::from_num(1)));
    /// let partition = Partition::<String>::create(io, "example repo", None);
    /// ```
    pub fn create<'a>(mut io: Box<PartIO>, name: &str,
            user: Option<&mut UserFields>) -> Result<Partition<E>>
    {
        try!(validate_repo_name(name));
        let ss = 0;
        let part_id = io.part_id();
        info!("Creating partiton {}; writing snapshot {}", part_id, ss);
        
        let state = PartState::new(part_id);
        let header = FileHeader {
            ftype: FileType::Snapshot(0),
            name: name.to_string(),
            part_id: Some(part_id),
            user: user.map_or(vec![], |u| u.write_user_fields(part_id, false)),
        };
        if let Some(mut writer) = try!(io.new_ss(ss)) {
            try!(write_head(&header, &mut writer));
            try!(write_snapshot(&state, &mut writer));
        } else {
            return make_io_err(ErrorKind::AlreadyExists, "snapshot already exists");
        }
        
        let mut part = Partition {
            io: io,
            repo_name: header.name,
            part_id: part_id,
            ss0: ss,
            ss1: ss + 1,
            ss_commits: 0,
            ss_edits: 0,
            states: HashIndexed::new(),
            parents: HashSet::new(),
            tips: HashSet::new(),
            unsaved: VecDeque::new(),
        };
        part.tips.insert(state.statesum().clone());
        part.states.insert(state);
        
        Ok(part)
    }
    
    /// Open a partition, assigning an IO provider (this can only be done at
    /// time of creation).
    /// 
    /// The partition will not be *ready to use* until data is loaded with one
    /// of the load operations. Until then most operations will fail.
    /// 
    /// If the repository name is known (e.g. from another partition), then
    /// setting this with `set_repo_name()` will ensure that the value is
    /// checked when loading files.
    /// 
    /// Example:
    /// 
    /// ```no_run
    /// use std::path::Path;
    /// use pippin::Partition;
    /// use pippin::discover;
    /// 
    /// let path = Path::new("./my-partition");
    /// let io = discover::part_from_path(path, None).unwrap();
    /// let partition = Partition::<String>::open(Box::new(io));
    /// ```
    pub fn open(io: Box<PartIO>) -> Result<Partition<E>> {
        let part_id = io.part_id();
        trace!("Opening partition {}", part_id);
        Ok(Partition {
            io: io,
            repo_name: "".to_string() /*temporary value; checked before usage elsewhere*/,
            part_id: part_id,
            ss0: 0,
            ss1: 0,
            ss_commits: 0,
            ss_edits: 0,
            states: HashIndexed::new(),
            parents: HashSet::new(),
            tips: HashSet::new(),
            unsaved: VecDeque::new(),
        })
    }
    
    /// Set the repo name. This is left empty by `open()`. Once set,
    /// partition operations will fail when loading a file with a different
    /// name.
    /// 
    /// This will fail if the repo name has already been set *and* is not
    /// equal to the `repo_name` parameter.
    pub fn set_repo_name(&mut self, repo_name: &str) -> Result<()> {
        if self.repo_name.len() == 0 {
            self.repo_name = repo_name.to_string();
        } else if self.repo_name != repo_name {
            return OtherError::err("repository name does not match when loading (wrong repo?)");
        }
        Ok(())
    }
    
    /// Get the repo name.
    /// 
    /// If this partition was created with `create()`, not `new()`, and no
    /// partition has been loaded yet, then this function will read a snapshot
    /// file header in order to find this name.
    /// 
    /// Returns the repo_name on success. Fails if it cannot read a header.
    pub fn get_repo_name(&mut self) -> Result<&str> {
        if self.repo_name.len() > 0 {
            return Ok(&self.repo_name);
        }
        for ss in (0 .. self.io.ss_len()).rev() {
            if let Some(mut ssf) = try!(self.io.read_ss(ss)) {
                let header = try!(read_head(&mut *ssf));
                try!(Self::verify_head(&header, &mut self.repo_name, self.part_id));
                return Ok(&self.repo_name);
            }
        }
        return OtherError::err("no snapshot found for first partition");
    }
    
    /// Load all history. Shortcut for `load_range(0, usize::MAX, user)`.
    pub fn load_all(&mut self, user: Option<&mut UserFields>) -> Result<()> {
        self.load_range(0, usize::MAX, user)
    }
    /// Load latest state from history (usually including some historical
    /// data). Shortcut for `load_range(usize::MAX, usize::MAX, user)`.
    pub fn load_latest(&mut self, user: Option<&mut UserFields>) -> Result<()> {
        self.load_range(usize::MAX, usize::MAX, user)
    }
    
    /// Load snapshots `ss` where `ss0 <= ss < ss1`, and all log files for each
    /// snapshot loaded. If `ss0` is beyond the latest snapshot found, it will
    /// be reduced to the number of the last snapshot. `ss1` may be large. For
    /// example, `(0, usize::MAX)` means load everything available and
    /// `(usize::MAX, usize::MAX)` means load only the latest state.
    /// 
    /// Special behaviour: if some snapshots are already loaded and the range
    /// does not overlap with this range, all snapshots in between will be
    /// loaded.
    /// 
    /// The `user` parameter allows any headers read to be examined. If `None`
    /// is passed, these fields are simply ignored.
    pub fn load_range(&mut self, ss0: usize, ss1: usize,
            mut user: Option<&mut UserFields>) -> Result<()>
    {
        // We have to consider several cases: nothing previously loaded, that
        // we're loading data older than what was previously loaded, or newer,
        // or even overlapping. The algorithm we use is:
        // 
        //  while ss0 > 0 and not has_ss(ss0), ss0 -= 1
        //  if ss0 == 0 and not has_ss(0), assume initial state
        //  for ss in ss0..ss1:
        //      if this snapshot was already loaded, skip
        //      load snapshot if found, skip if not
        //      load all logs found and rebuild states, aborting if parents are missing
        
        // Input arguments may be greater than the available snapshot numbers. Clamp:
        let ss_len = self.io.ss_len();
        let mut ss0 = min(ss0, if ss_len > 0 { ss_len - 1 } else { ss_len });
        let mut ss1 = min(ss1, ss_len);
        // If data is already loaded, we must load snapshots between it and the new range too:
        if self.ss1 > self.ss0 {
            if ss0 > self.ss1 { ss0 = self.ss1; }
            if ss1 < self.ss0 { ss1 = self.ss0; }
        }
        // If snapshot files are missing, we need to load older files:
        while ss0 > 0 && !self.io.has_ss(ss0) { ss0 -= 1; }
        info!("Loading partition {} data with snapshot range ({}, {})", self.part_id, ss0, ss1);
        
        if ss0 == 0 && !self.io.has_ss(ss0) {
            assert!(self.states.is_empty());
            // No initial snapshot; assume a blank state
            let state = PartState::new(self.part_id);
            self.tips.insert(state.statesum().clone());
            self.states.insert(state);
        }
        
        let mut require_ss = false;
        for ss in ss0..ss1 {
            // If already loaded, skip this snapshot:
            if self.ss0 <= ss && ss < self.ss1 { continue; }
            let at_tip = ss >= self.ss1;
            
            if let Some(mut r) = try!(self.io.read_ss(ss)) {
                let head = try!(read_head(&mut r));
                try!(Self::verify_head(&head, &mut self.repo_name, self.part_id));
                let file_ver = head.ftype.ver();
                if let Some(ref mut u) = user {
                    u.read_user_fields(head.user, self.part_id, false);
                }
                
                let state = try!(read_snapshot(&mut r, self.part_id, file_ver));
                
                if !self.parents.contains(state.statesum()) {
                    self.tips.insert(state.statesum().clone());
                }
                for parent in state.parents() {
                    if !self.states.contains(parent) {
                        self.parents.insert(parent.clone());
                    }
                }
                self.states.insert(state);
                require_ss = false;
                if at_tip {
                    // reset snapshot policy
                    self.ss_commits = 0;
                    self.ss_edits = 0;
                }
            } else {
                // Missing snapshot; if at head require a new one
                require_ss = at_tip;
            }
            
            let mut queue = vec![];
            for cl in 0..self.io.ss_cl_len(ss) {
                if let Some(mut r) = try!(self.io.read_ss_cl(ss, cl)) {
                    let head = try!(read_head(&mut r));
                    try!(Self::verify_head(&head, &mut self.repo_name, self.part_id));
                    if let Some(ref mut u) = user {
                        u.read_user_fields(head.user, self.part_id, true);
                    }
                    try!(read_log(&mut r, &mut queue));
                }
            }
            for commit in queue {
                try!(self.add_commit(commit));
            }
            if at_tip {
                self.ss1 = ss + 1;
            }
        }
        
        if ss0 < self.ss0 {
            // Older history was loaded. In this case we can only update ss0
            // once all older snapshots have been loaded. If there was a failure
            // and retry, some snapshots could be reloaded unnecessarily.
            self.ss0 = ss0;
        }
        assert!(self.ss0 <= ss1 && ss1 <= self.ss1);
        
        if require_ss {
            // require a snapshot
            self.ss_commits = 0x10_0000;
        }
        Ok(())
    }
    
    /// Returns true when elements have been loaded (i.e. there is at least one
    /// tip; see also `is_ready` and `merge_required`).
    pub fn is_loaded(&self) -> bool {
        self.tips.len() > 0
    }
    
    /// Returns true when ready for use (this is equivalent to
    /// `self.is_loaded() && !self.merge_required()`, i.e. there is exactly
    /// one tip).
    pub fn is_ready(&self) -> bool {
        self.tips.len() == 1
    }
    
    /// Returns true while a merge is required (i.e. there is more than one
    /// tip).
    /// 
    /// Returns false if not ready or no tip is found as well as when a single
    /// tip is present and ready to use.
    pub fn merge_required(&self) -> bool {
        self.tips.len() > 1
    }
    
    /// Verify values in a header match those we expect.
    /// 
    /// This function is called for every file loaded. It does not take self as
    /// an argument, since it is called in situations where self.io is in use.
    pub fn verify_head(head: &FileHeader, self_name: &mut String,
        self_partid: PartId) -> Result<()>
    {
        if self_name.len() == 0 {
            *self_name = head.name.clone();
        } else if *self_name != head.name{
            return OtherError::err("repository name does not match when loading (wrong repo?)");
        }
        if let Some(h_pid) = head.part_id {
            if self_partid != h_pid {
                return OtherError::err("partition identifier differs from previous value");
            }
        }
        Ok(())
    }
    
    /// Unload data from memory. Note that unless `force == true` the operation
    /// will fail if any changes have not yet been saved to disk.
    /// 
    /// Returns true if data was unloaded, false if not (implies `!force` and 
    /// that unsaved changes exist).
    pub fn unload(&mut self, force: bool) -> bool {
        trace!("Unloading partition {} data", self.part_id);
        if force || self.unsaved.is_empty() {
            self.states.clear();
            self.parents.clear();
            self.tips.clear();
            true
        } else {
            false
        }
    }
    
    /// Consume the `Partition` and return the held `PartIO`.
    /// 
    /// This destroys all states held internally, but states may be cloned
    /// before unwrapping. Since `Element`s are copy-on-write, cloning
    /// shouldn't be too expensive.
    pub fn unwrap_io(self) -> Box<PartIO> {
        self.io
    }
    
    /// Get the partition's number
    pub fn part_id(&self) -> PartId {
        self.part_id
    }
}

// Methods accessing or modifying a partition's data
impl<E: ElementT> Partition<E> {
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
    
    /// Get the set of all tips. Is empty before loading and has more than one
    /// entry when `merge_required()`. May be useful for merging.
    pub fn tips(&self) -> &HashSet<Sum> {
        &self.tips
    }
    
    /// Get a reference to the PartState of the current tip. You can read
    /// this directly or make a clone in order to make your modifications.
    /// 
    /// This operation will fail if no data has been loaded yet or a merge is
    /// required.
    /// 
    /// The operation requires some copying but uses copy'c,d-on-write elements
    /// internally. This copy is needed to create a commit from the diff of the
    /// last committed state and the new state.
    pub fn tip(&self) -> result::Result<&PartState<E>, TipError> {
        Ok(&self.states.get(try!(self.tip_key())).unwrap())
    }
    
    /// Iterate over all states known. If `self.load(true)` was used to load
    /// all history available, this will include all historical states found
    /// (which may still not be all history), otherwise if `self.load(false)`
    /// was used, only some recent states (in theory, everything back to the
    /// last snapshot at time of loading) will be present.
    /// 
    /// Items are unordered (actually, they follow the order of an internal
    /// hash map, which is randomised and usually different each time the
    /// program is loaded).
    pub fn states(&self) -> StateIter<E> {
        StateIter { iter: self.states.iter(), tips: &self.tips }
    }
    
    /// Get a read-only reference to a state by its statesum, if found.
    /// 
    /// If you want to keep a copy, clone it.
    pub fn state(&self, key: &Sum) -> Option<&PartState<E>> {
        self.states.get(key)
    }
    
    /// Try to find a state given a string representation of the key (as a byte array).
    /// 
    /// Like git, we accept partial keys (so long as they uniquely resolve a key).
    pub fn state_from_string(&self, string: String) -> Result<&PartState<E>, MatchError> {
        let string = string.to_uppercase().replace(" ", "");
        let mut matching: Option<&Sum> = None;
        for state in self.states.iter() {
            if state.statesum().matches_string(&string.as_bytes()) {
                if let Some(prev) = matching {
                    return Err(MatchError::MultiMatch(
                        prev.as_string(false), state.statesum().as_string(false)));
                } else {
                    matching = Some(state.statesum());
                }
            }
        }
        if let Some(m) = matching {
            Ok(self.states.get(&m).unwrap())
        } else {
            Err(MatchError::NoMatch)
        }
    }
    
    /// Merge all latest states into a single tip.
    /// This is a convenience wrapper around `merge_two(...)`.
    /// 
    /// This works through all 'tip' states in an order determined by a
    /// `HashSet`'s random keying, thus the exact result may not be repeatable
    /// if the program were run multiple times with the same initial state.
    /// 
    /// If `auto_load` is true, additional history will be loaded as necessary
    /// to find a common ancestor.
    pub fn merge<S: TwoWaySolver<E>>(&mut self, solver: &S, auto_load: bool,
        make_meta: Option<&MakeMeta>) -> Result<()>
    {
        while self.tips.len() > 1 {
            let (tip1, tip2): (Sum, Sum) = {
                let mut iter = self.tips.iter();
                let tip1 = iter.next().unwrap();
                let tip2 = iter.next().unwrap();
                (tip1.clone(), tip2.clone())
            };
            trace!("Partition {}: attempting merge of tips {} and {}", self.part_id, &tip1, &tip2);
            let c = try!(self.merge_two(&tip1, &tip2, auto_load))
                    .solve_inline(solver).make_commit(make_meta);
            if let Some(commit) = c {
                trace!("Pushing merge commit: {} ({} changes)",
                        commit.statesum(), commit.num_changes());
                try!(self.push_commit(commit));
            } else {
                return Err(box MergeError::NotSolved);
            }
        }
        Ok(())
    }
    
    /// Creates a `TwoWayMerge` for two given states (presumably tip states,
    /// but not required).
    /// 
    /// In order to merge multiple tips, either use `self.merge(solver)` or
    /// proceed step-by-step:
    /// 
    /// *   find two tip states (probably from `self.tips()`)
    /// *   call `part.merge_two(tip1, tip2)` to get a `merger`
    /// *   call `merger.solve(solver)` or solve manually
    /// *   call `merger.make_commit(...)` to obtain a `commit`
    /// *   call `part.push_commit(commit)`
    /// *   repeat while `part.merge_required()` remains true
    /// 
    /// This is not eligant, but provides the user full control over the merge.
    /// Alternatively, use `self.merge(solver)`.
    /// 
    /// If `auto_load` is true, additional history will be loaded as necessary
    /// to find a common ancestor.
    pub fn merge_two(&mut self, tip1: &Sum, tip2: &Sum, auto_load: bool) ->
            Result<TwoWayMerge<E>>
    {
        let common;
        loop {
            match self.latest_common_ancestor(tip1, tip2) {
                Ok(sum) => {
                    common = sum;
                    break;
                },
                Err(MergeError::NoCommonAncestor) if auto_load && self.ss0 > 0 => {
                    let ss0 = self.ss0;
                    try!(self.load_range(ss0 - 1, ss0, None));
                    continue;
                },
                Err(e) => {
                    return Err(box e);
                }
            }
        }
        let s1 = try!(self.states.get(tip1).ok_or(MergeError::NoState));
        let s2 = try!(self.states.get(tip2).ok_or(MergeError::NoState));
        let s3 = try!(self.states.get(&common).ok_or(MergeError::NoState));
        Ok(TwoWayMerge::new(s1, s2, s3))
    }
    
    // #0003: allow getting a reference to other states listing snapshots,
    // commits, getting non-current states and getting diffs.
    
    /// This adds a new commit to the list waiting to be written and updates
    /// the states and 'tips' stored internally by creating a new state from
    /// the commit.
    /// 
    /// Mutates the commit in the (very unlikely) case that its statesum
    /// clashes with another commit whose data is different.
    /// 
    /// Fails if the commit's parent is not found or the patch cannot be
    /// applied to it. In this case the commit is lost, but presumably either
    /// there was a programmatic error or memory corruption for this to occur.
    /// 
    /// Returns `Ok(true)` on success or `Ok(false)` if the commit matches an
    /// already known state.
    pub fn push_commit(&mut self, commit: Commit<E>) -> Result<bool, PatchOp> {
        let state = {
            let parent = try!(self.states.get(commit.first_parent())
                .ok_or(PatchOp::NoParent));
            try!(PartState::from_state_commit(parent, &commit))
        };  // end borrow on self (from parent)
        Ok(self.add_pair(commit, state))
    }
    
    /// Add a new state, assumed to be derived from an existing known state.
    /// 
    /// This creates a commit from the given state, converts the `MutPartState`
    /// to a `PartState` and adds it to the list of internal states, and
    /// updates the tip. The commit is added to the internal list
    /// waiting to be written to permanent storage (see `write()`).
    /// 
    /// We assume there are no extra parents; merges should be pushed via
    /// `push_commit` instead.
    /// 
    /// Mutates the commit in the (very unlikely) case that its statesum
    /// clashes with another commit whose data is different.
    /// 
    /// Returns `Ok(true)` on success, or `Ok(false)` if the state matches its
    /// parent (i.e. hasn't been changed) or another already known state.
    pub fn push_state(&mut self, state: MutPartState<E>,
            make_meta: Option<&MakeMeta>) -> Result<bool, PatchOp>
    {
        let parent_sum = state.parent().clone();
        let new_state = PartState::from_mut(state, make_meta);
        
        // #0019: Commit::from_diff compares old and new states and code be slow.
        // #0019: Instead, we could record each alteration as it happens.
        Ok(if let Some(commit) =
                Commit::from_diff(
                    try!(self.states.get(&parent_sum).ok_or(PatchOp::NoParent)),
                    &new_state)
            {
                self.add_pair(commit, new_state)
            } else {
                false
            }
        )
    }
    
    /// This will write all unsaved commits to a log on the disk. Does nothing
    /// if there are no queued changes.
    /// 
    /// If `fast` is true, no further actions will happen, otherwise required
    /// maintenance operations will be carried out (e.g. creating a new
    /// snapshot when the current commit-log is long).
    /// 
    /// `user` allows extra data to be written to file headers.
    /// 
    /// Returns true if any commits were written (i.e. unsaved commits
    /// were found). Returns false if nothing needed doing.
    /// 
    /// Note that writing to disk can fail. In this case it may be worth trying
    /// again.
    pub fn write(&mut self, fast: bool, mut user: Option<&mut UserFields>) -> Result<bool> {
        // First step: write commits
        let has_changes = !self.unsaved.is_empty();
        if has_changes {
            let part_id = self.part_id;
            trace!("Partition {}: writing {} commits to log",
                part_id, self.unsaved.len());
            
            // #0012: extend existing logs instead of always writing a new log file.
            let mut cl_num = self.io.ss_cl_len(self.ss1 - 1);
            loop {
                if let Some(mut writer) = try!(self.io.new_ss_cl(self.ss1 - 1, cl_num)) {
                    // Write a header since this is a new file:
                    let header = FileHeader {
                        ftype: FileType::CommitLog(0),
                        name: self.repo_name.clone(),
                        part_id: Some(part_id),
                        user: user.as_mut().map_or(vec![], |u| u.write_user_fields(part_id, true)),
                    };
                    try!(write_head(&header, &mut writer));
                    try!(start_log(&mut writer));
                    
                    // Now write commits:
                    while !self.unsaved.is_empty() {
                        // We try to write the commit, then when successful remove it
                        // from the list of 'unsaved' commits.
                        try!(write_commit(&self.unsaved.front().unwrap(), &mut writer));
                        self.unsaved.pop_front().expect("pop_front");
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
            if self.is_ready() && self.io.want_snapshot(self.ss_commits, self.ss_edits) {
                try!(self.write_snapshot(user));
            }
        }
        
        Ok(has_changes)
    }
    
    /// Write a new snapshot from the tip.
    /// 
    /// Normally you can just call `write()` and let the library figure out
    /// when to write a new snapshot, though you can also call this directly.
    /// 
    /// Does nothing when `tip()` fails (returning `Ok(())`).
    /// 
    /// `user` allows extra data to be written to file headers.
    pub fn write_snapshot(&mut self, user: Option<&mut UserFields>) -> Result<()> {
        // fail early if not ready:
        let tip_key = try!(self.tip_key()).clone();
        let part_id = self.part_id;
        
        let mut ss_num = self.ss1;
        loop {
            // Try to get a writer for this snapshot number:
            if let Some(mut writer) = try!(self.io.new_ss(ss_num)) {
                info!("Partition {}: writing snapshot {}: {}",
                    part_id, ss_num, tip_key);
                
                let header = FileHeader {
                    ftype: FileType::Snapshot(0),
                    name: self.repo_name.clone(),
                    part_id: Some(part_id),
                    user: user.map_or(vec![], |u| u.write_user_fields(part_id, false)),
                };
                try!(write_head(&header, &mut writer));
                try!(write_snapshot(self.states.get(&tip_key).unwrap(), &mut writer));
                self.ss1 = ss_num + 1;
                // reset snapshot policy:
                self.ss_commits = 0;
                self.ss_edits = 0;
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

// Internal support functions
impl<E: ElementT> Partition<E> {
    // Take self and two sums. Return a copy of a key to avoid lifetime issues.
    fn latest_common_ancestor(&self, k1: &Sum, k2: &Sum) -> Result<Sum, MergeError> {
        // #0019: there are multiple strategies here; we just find all
        // ancestors of one, then of the other. This simplifies lopic.
        let mut a1 = HashSet::new();
        
        let mut next = VecDeque::new();
        next.push_back(k1);
        loop {
            let k = match next.pop_back() {
                Some(k) => k,
                None => { break; }
            };
            if a1.contains(k) { continue; }
            a1.insert(k);
            if let Some(state) = self.states.get(k) {
                for p in state.parents() {
                    next.push_back(p);
                }
            }
        }
        
        // We track ancestors of k2 just to check we don't end up in a loop.
        let mut a2 = HashSet::new();
        
        // next is empty
        next.push_back(k2);
        loop {
            let k = match next.pop_back() {
                Some(k) => k,
                None => { break; }
            };
            if a2.contains(k) { continue; }
            a2.insert(k);
            if a1.contains(k) {
                return Ok(k.clone());
            }
            if let Some(state) = self.states.get(k) {
                for p in state.parents() {
                    next.push_back(p);
                }
            }
        }
        
        Err(MergeError::NoCommonAncestor)
    }
    
    /// Add a state, assuming that this isn't a new one (i.e. it's been loaded
    /// from a file and doesn't need to be saved).
    /// 
    /// Unlike add_pair, mutating isn't possible, so this just returns false
    /// if the sum is known without checking whether the state is different.
    /// 
    /// `n_edits` is the number of changes in a commit. Normally the state is
    /// created from a commit, and `n_edits = commit.num_changes()`; in the
    /// case the state is a loaded snapshot the "snapshot policy" should be
    /// reset afterwards, hence n_edits does not matter.
    /// 
    /// Returns true unless the given state's sum equals an existing one.
    fn add_state(&mut self, state: PartState<E>, n_edits: usize) {
        trace!("Partition {}: add state {}", self.part_id, state.statesum());
        if self.states.contains(state.statesum()) {
            trace!("Partition {} already contains state {}", self.part_id, state.statesum());
            return;
        }
        
        for parent in state.parents() {
            // Remove from 'tips' if it happened to be there:
            self.tips.remove(parent);
            // Add to 'parents' if not in 'states':
            if !self.states.contains(parent) {
                self.parents.insert(parent.clone());
            }
        }
        // We know from above 'state' is not in 'self.states'; if it's not in
        // 'self.parents' either then it must be a tip:
        if !self.parents.contains(state.statesum()) {
            self.ss_commits += 1;
            self.ss_edits += n_edits;
            self.tips.insert(state.statesum().clone());
        }
        self.states.insert(state);
    }
    
    /// Creates a state from the commit and adds to self. Updates tip if this
    /// state is new.
    pub fn add_commit(&mut self, commit: Commit<E>) -> Result<(), PatchOp> {
        if self.states.contains(commit.statesum()) { return Ok(()); }
        
        let state = {
            let parent = try!(self.states.get(commit.first_parent())
                .ok_or(PatchOp::NoParent));
            try!(PartState::from_state_commit(parent, &commit))
        };  // end borrow on self (from parent)
        self.add_state(state, commit.num_changes());
        Ok(())
    }
    
    /// Add a paired commit and state, asserting that the checksums match and
    /// the parent state is present. Also add to the queue awaiting `write()`.
    /// 
    /// If an element with the states's statesum already exists and differs
    /// from the state passed, the state and commit passed will be mutated to
    /// achieve a unique statesum.
    /// 
    /// Returns true unless the given state (including metadata) equals a
    /// stored one (in which case nothing happens and false is returned).
    fn add_pair(&mut self, mut commit: Commit<E>, mut state: PartState<E>) -> bool {
        trace!("Partition {}: add commit {}", self.part_id, commit.statesum());
        assert_eq!(commit.parents(), state.parents());
        assert_eq!(commit.statesum(), state.statesum());
        assert!(self.states.contains(commit.first_parent()));
        
        while let Some(ref old_state) = self.states.get(state.statesum()) {
            if state == **old_state {
                trace!("Partition {} already contains commit {}", self.part_id, commit.statesum());
                return false;
            } else {
                commit.mutate_meta(state.mutate_meta());
                trace!("Partition {}: mutated commit to {}", self.part_id, commit.statesum());
            }
        }
        
        self.add_state(state, commit.num_changes());
        self.unsaved.push_back(commit);
        true
    }
}

/// Wrapper around a `PartState<E>`. Dereferences to this type.
pub struct StateItem<'a, E: ElementT+'a> {
    state: &'a PartState<E>,
    tips: &'a HashSet<Sum>,
}
impl<'a, E: ElementT+'a> StateItem<'a, E> {
    /// Returns true if and only if this state is a tip state (i.e. is not the
    /// parent of any other state).
    /// 
    /// There is exactly one tip state unless a merge is required or no states
    /// are loaded.
    pub fn is_tip(&self) -> bool {
        self.tips.contains(self.state.statesum())
    }
}
impl<'a, E: ElementT+'a> Deref for StateItem<'a, E> {
    type Target = PartState<E>;
    fn deref(&self) -> &Self::Target {
        self.state
    }
}

/// Iterator over a partition's (historical or current) states
pub struct StateIter<'a, E: ElementT+'a> {
    iter: Iter<'a, PartState<E>, Sum, PartStateSumComparator>,
    tips: &'a HashSet<Sum>,
}
impl<'a, E: ElementT+'a> Iterator for StateIter<'a, E> {
    type Item = StateItem<'a, E>;
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|item|
            StateItem {
                state: item,
                tips: self.tips,
            }
        )
    }
    fn size_hint(&self) -> (usize, Option<usize>) { self.iter.size_hint() }
}


#[cfg(test)]
mod tests {
    use super::*;
    use commit::{Commit};
    use PartId;
    
    #[test]
    fn commit_creation_and_replay(){
        use {PartId};
        use std::rc::Rc;
        
        let p = PartId::from_num(1);
        let mut queue = vec![];
        
        let insert = |state: &mut MutPartState<_>, num, string: &str| -> Result<_, _> {
            state.insert_with_id(p.elt_id(num), Rc::new(string.to_string()))
        };
        
        let mut state = PartState::new(p).clone_mut();
        insert(&mut state, 1, "one").unwrap();
        insert(&mut state, 2, "two").unwrap();
        let state_a = PartState::from_mut(state, None);
        
        let mut state = state_a.clone_mut();
        insert(&mut state, 3, "three").unwrap();
        insert(&mut state, 4, "four").unwrap();
        insert(&mut state, 5, "five").unwrap();
        let state_b = PartState::from_mut(state, None);
        let commit = Commit::from_diff(&state_a, &state_b).unwrap();
        queue.push(commit);
        
        let mut state = state_b.clone_mut();
        insert(&mut state, 6, "six").unwrap();
        insert(&mut state, 7, "seven").unwrap();
        state.remove(p.elt_id(4)).unwrap();
        state.replace(p.elt_id(3), "half six".to_string()).unwrap();
        let state_c = PartState::from_mut(state, None);
        let commit = Commit::from_diff(&state_b, &state_c).unwrap();
        queue.push(commit);
        
        let mut state = state_c.clone_mut();
        insert(&mut state, 8, "eight").unwrap();
        insert(&mut state, 4, "half eight").unwrap();
        let state_d = PartState::from_mut(state, None);
        let commit = Commit::from_diff(&state_c, &state_d).unwrap();
        queue.push(commit);
        
        let io = box DummyPartIO::new(PartId::from_num(1));
        let mut part = Partition::create(io, "replay part", None).unwrap();
        part.add_state(state_a, 0);
        for commit in queue {
            part.push_commit(commit).unwrap();
        }
        
        assert_eq!(part.tips.len(), 1);
        let replayed_state = part.tip().unwrap();
        assert_eq!(*replayed_state, state_d);
    }
    
    #[test]
    fn on_new_partition() {
        let io = box DummyPartIO::new(PartId::from_num(7));
        let mut part = Partition::<String>::create(io, "on_new_partition", None)
                .expect("partition creation");
        assert_eq!(part.tips.len(), 1);
        
        let state = part.tip().expect("getting tip").clone_mut();
        assert_eq!(part.push_state(state, None).expect("committing"), false);
        
        let mut state = part.tip().expect("getting tip").clone_mut();
        assert!(!state.any_avail());
        
        let elt1 = "This is element one.".to_string();
        let elt2 = "Element two data.".to_string();
        let e1id = state.insert(elt1).expect("inserting elt");
        let e2id = state.insert(elt2).expect("inserting elt");
        
        assert_eq!(part.push_state(state, None).expect("comitting"), true);
        assert_eq!(part.unsaved.len(), 1);
        assert_eq!(part.states.len(), 2);
        let key = part.tip().expect("tip").statesum().clone();
        {
            let state = part.state(&key).expect("getting state by key");
            assert!(state.is_avail(e1id));
            assert_eq!(state.get(e2id), Ok(&"Element two data.".to_string()));
        }   // `state` goes out of scope
        assert_eq!(part.tips.len(), 1);
        let state = part.tip().expect("getting tip").clone_mut();
        assert_eq!(*state.parent(), key);
        
        assert_eq!(part.push_state(state, None).expect("committing"), false);
    }
}
