/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Pippin: commit structs and functionality

use std::collections::{HashMap, hash_map};
use std::clone::Clone;
use std::rc::Rc;
use std::u32;
use std::cmp::max;

use chrono::{DateTime, NaiveDateTime, UTC};

use detail::readwrite::CommitReceiver;
use {PartState, MutPartState, MutState};
use {ElementT, EltId, PartId, Sum};
use error::{Result, PatchOp, ElementOp};


/// Closure type to make metadata.
/// 
/// PartId is the identifier of the partition receiving the change. The vector
/// of commit-metas references each parent commit's metadata.
pub type MakeMeta = Fn(PartId, Vec<&CommitMeta>) -> CommitMeta;

/// Extra data about a commit, version 1. This type should remain fixed, unlike
/// `CommitMeta`.
#[derive(Eq, PartialEq, Clone, Debug)]
pub struct CommitMeta1 {
    /// Commit number. First (real) commit has number 1, each subsequent commit
    /// has max-parent-number + 1. Can be used to identify commits but is not
    /// necessarily unique.
    pub number: u32,
    /// Time of commit creation
    /// 
    /// This is a UNIX time-stamp: the number of non-leap seconds since January
    /// 1, 1970 0:00:00 UTC. See `date_time()` or use
    /// `chrono::NaiveDateTime::from_timestamp` directly.
    /// 
    /// In rare cases this may be zero. 
    pub timestamp: i64,
    /// Extra metadata (e.g. author, comments).
    pub extra: Option<String>,
}

// Private enum of all meta versions; used so that fields of CommitMeta are not
// public.
#[derive(Debug, Clone)]
enum MetaInternal {
    Meta1(CommitMeta1)
}
// This is implemented manually so that cross-version "equality" does get
// thought about. I'm not sure where it's needed or what it should do yet.
impl PartialEq for MetaInternal {
    fn eq(&self, rhs: &Self) -> bool {
        match rhs {
            &MetaInternal::Meta1(ref r) => {
                match self {
                    &MetaInternal::Meta1(ref m) => m == r,
                }
            },
        }
    }
}

/// A abstraction over all metadata versions (this is what you normally use,
/// except for creation, when you can use one of the versioned types).
/// 
/// The internal details may change over time, and new public methods may be
/// added.
/// 
/// Creation should be via one of the named constructors (e.g.
/// `CommitMeta::now_empty()`) or via a versioned struct, e.g.
/// 
/// ```rust
/// use pippin::commit::{CommitMeta, CommitMeta1};
/// 
/// let meta: CommitMeta = CommitMeta1 {
///         number: 7,
///         timestamp: CommitMeta::timestamp_now(),
///         extra: None
///     }.into();
/// ```
/// 
/// Fields may be retrieved via getters, e.g. `meta.number()`.
#[derive(Debug, PartialEq, Clone)]
pub struct CommitMeta {
    meta: MetaInternal
}

impl CommitMeta {
    /// Create an instance applicable to a new empty partition.
    /// 
    /// Assigns a timestamp as of *now* (via `Self::timestamp_now()`).
    pub fn now_empty() -> CommitMeta {
        Self::now_with(0, None)
    }
    /// Create an instance with provided number and optional extra data.
    /// 
    /// Assigns a timestamp as of *now* (via `Self::timestamp_now()`).
    pub fn now_with(number: u32, extra: Option<String>) -> CommitMeta {
        Self::from(CommitMeta1 {
            number: number,
            timestamp: Self::timestamp_now(),
            extra: extra,
        })
    }
    /// Create a default CommitMeta for a commit, given a list of the metadata
    /// of parent commits (one parent for a normal commit, more for a merge,
    /// none for the initial state).
    pub fn make_default(parents: Vec<&CommitMeta>) -> CommitMeta {
        let number = parents.iter().fold(0, |prev, &m| max(prev, m.next_number()));
        Self::now_with(number, None)
    }
    
    /// Utility method to create a timestamp representing this moment.
    /// 
    /// Code is `UTC::now().timestamp()`, using `chrono::UTC`.
    pub fn timestamp_now() -> i64 {
        UTC::now().timestamp()
    }
    
    /// Get the commit's timestamp
    pub fn timestamp(&self) -> i64 {
        match &self.meta {
            &MetaInternal::Meta1(ref m) => m.timestamp,
        }
    }
    /// Convert the internal timestamp to a `chrono::DateTime`.
    pub fn date_time(&self) -> DateTime<UTC> {
        DateTime::<UTC>::from_utc(
                NaiveDateTime::from_timestamp(self.timestamp(), 0),
                UTC)
    }
    
    /// Get the commit's number
    pub fn number(&self) -> u32 {
        match &self.meta {
            &MetaInternal::Meta1(ref m) => m.number,
        }
    }
    /// Get the next number (usually current + 1)
    pub fn next_number(&self) -> u32 {
        let n = self.number();
        if n < u32::MAX { n + 1 } else { u32::MAX }
    }
    /// Increment the commit number via `number = next_number()`.
    /// This is for internal usage and not guaranteed to remain.
    pub fn incr_number(&mut self) {
        let n = self.next_number();
        match &mut self.meta {
            &mut MetaInternal::Meta1(ref mut m) => { m.number = n; },
        }
    }
    
    /// Get the version of the internal metadata format. I.e. for `CommitMeta1`
    /// this is 1, and so on.
    pub fn ver(&self) -> u32 {
        match self.meta {
            MetaInternal::Meta1(_) => 1,
        }
    }
    
    /// Get the commit's extra data. There are no guarantees to what this will
    /// look like in future meta-data versions, or even if it will be present.
    pub fn extra(&self) -> Option<&String> {
        match &self.meta {
            &MetaInternal::Meta1(ref m) => m.extra.as_ref(),
        }
    }
}

impl From<CommitMeta1> for CommitMeta {
    fn from(m: CommitMeta1) -> Self {
        CommitMeta { meta: MetaInternal::Meta1(m) }
    }
}

/// Holds a set of commits, ordered by insertion order.
/// This is only really needed for readwrite::read_log().
pub struct CommitQueue<E: ElementT> {
    // These must be ordered. We only ever access by iteration so a `Vec` is
    // fine.
    commits: Vec<Commit<E>>
}

impl<E: ElementT> CommitQueue<E> {
    /// Create an empty CommitQueue.
    pub fn new() -> CommitQueue<E> {
        CommitQueue { commits: Vec::new() }
    }
    /// Destroy the CommitQueue, returning the internal list of commits
    pub fn unwrap(self) -> Vec<Commit<E>> {
        self.commits
    }
}

impl<E: ElementT> CommitReceiver<E> for CommitQueue<E> {
    /// Implement function required by readwrite::read_log().
    fn receive(&mut self, commit: Commit<E>) -> bool {
        self.commits.push(commit);
        true    // continue reading to EOF
    }
}


/// A commit: a set of changes.
/// 
/// The number of parents is at least one; where more this is a merge commit.
#[derive(PartialEq, Debug)]
pub struct Commit<E: ElementT> {
    /// Expected resultant state sum; doubles as an ID.
    statesum: Sum,
    /// State sum (ID) of parent states. There must be at least one. Thes first
    /// state is the *primary parent* and is what the *changes* are relative
    /// to; the rest are simply "additional parents".
    parents: Vec<Sum>,
    /// Per-element changes
    changes: HashMap<EltId, EltChange<E>>,
    /// Meta-data
    meta: CommitMeta,
}

/// Per-element changes
#[derive(PartialEq, Debug)]
pub enum EltChange<E: ElementT> {
    /// Element was deleted
    Deletion,
    /// Element was added (full data)
    Insertion(Rc<E>),
    /// Element was replaced (full data)
    Replacement(Rc<E>),
    /// Element has been moved, must be removed from this partition; new identity mentioned
    MovedOut(EltId),
    /// Same as `MovedOut` except that the element has already been removed from the partition
    Moved(EltId),
}
impl<E: ElementT> EltChange<E> {
    /// Create an `Insertion`
    pub fn insertion(elt: Rc<E>) -> EltChange<E> {
        EltChange::Insertion(elt)
    }
    /// Create a `Replacement`
    pub fn replacement(elt: Rc<E>) -> EltChange<E> {
        EltChange::Replacement(elt)
    }
    /// Create a `Deletion`
    pub fn deletion() -> EltChange<E> {
        EltChange::Deletion
    }
    /// Create a note that the element has moved.
    /// 
    /// If `remove` is true, this also deletes the element from the state
    /// (otherwise it is assumed that the element has already been deleted).
    pub fn moved(new_id: EltId, remove: bool) -> EltChange<E> {
        match remove {
            true => EltChange::MovedOut(new_id),
            false => EltChange::Moved(new_id),
        }
    }
    /// Get `Some(elt)` if an element is contained, `None` otherwise
    pub fn element(&self) -> Option<&Rc<E>> {
        match self {
            &EltChange::Deletion => None,
            &EltChange::Insertion(ref elt) => Some(elt),
            &EltChange::Replacement(ref elt) => Some(elt),
            &EltChange::MovedOut(_) => None,
            &EltChange::Moved(_) => None,
        }
    }
    /// Get `Some(new_id)` if this is a "moved" change, else None
    pub fn moved_id(&self) -> Option<EltId> {
        match self {
            &EltChange::Deletion => None,
            &EltChange::Insertion(_) => None,
            &EltChange::Replacement(_) => None,
            &EltChange::MovedOut(id) => Some(id),
            &EltChange::Moved(id) => Some(id)
        }
    }
}

// —————  Commit operations  —————

impl<E: ElementT> Commit<E> {
    /// Create a commit from parts. It is suggested not to use this unless you
    /// are sure all sums are correct.
    /// 
    /// This panics if parents.len() == 0 or parents.len() >= 256.
    pub fn new_explicit(statesum: Sum, parents: Vec<Sum>,
            changes: HashMap<EltId, EltChange<E>>,
            meta: CommitMeta) -> Commit<E>
    {
        assert!(parents.len() >= 1 && parents.len() < 0x100);
        Commit { statesum: statesum, parents: parents, changes: changes,
                meta: meta }
    }
    
    /// Create a commit from an old state and a new state. Return the commit if
    /// there are any differences or None if the states are identical.
    /// 
    /// This is one of two ways to create a commit; the other would be to track
    /// changes to a state (possibly the latter is the more sensible approach
    /// for most applications).
    pub fn from_diff(old_state: &PartState<E>, new_state: &MutPartState<E>,
            make_meta: Option<&MakeMeta>) -> Option<Commit<E>>
    {
        let mut elt_map = new_state.elt_map().clone();
        let mut changes = HashMap::new();
        for (id, old_elt) in old_state.elt_map() {
            if let Some(new_elt) = elt_map.remove(id) {
                // #0019: should we compare sums? Would this be faster?
                if new_elt == *old_elt {
                    /* no change */
                } else {
                    changes.insert(*id, EltChange::replacement(new_elt));
                }
            } else {
                // not in new state: has been deleted
                changes.insert(*id, EltChange::deletion());
            }
        }
        let mut moved_map = new_state.moved_map().clone();
        for (id, new_id) in old_state.moved_map() {
            if let Some(new_id2) = moved_map.remove(&id) {
                if *new_id == new_id2 {
                    /* no change */
                } else {
                    changes.insert(*id, EltChange::moved(new_id2, false));
                }
            } else {
                // we seem to have forgotten that an element was moved
                // TODO: why? Should we track this so patches can make states *forget*?
                // TODO: should we warn about it?
            }
        }
        for (id, new_elt) in elt_map {
            changes.insert(id, EltChange::insertion(new_elt));
        }
        for (id, new_id) in moved_map {
            changes.insert(id, EltChange::moved(new_id, true));
        }
        
        if changes.is_empty() {
            None
        } else {
            let parents = vec![old_state.statesum().clone()];
            let metadata = if let Some(mm) = make_meta {
                mm(new_state.part_id(), vec![&old_state.meta()])
            } else {
                CommitMeta::make_default(vec![&old_state.meta()])
            };
            let metasum = Sum::state_meta_sum(new_state.part_id(),
                    &parents, &metadata);
            Some(Commit {
                statesum: new_state.elt_sum() ^ &metasum,
                parents: parents,
                changes: changes,
                meta: metadata,
            })
        }
    }
    
    /// Apply this commit to a given state, yielding a new state.
    /// 
    /// Fails if the given state's initial state-sum is not equal to this
    /// commit's parent or if there are any errors in applying this patch.
    pub fn apply(&self, parent: &PartState<E>) ->
            Result<PartState<E>, PatchOp>
    {
        if *parent.statesum() != self.parents[0] { return Err(PatchOp::WrongParent); }
        let mut mut_state = parent.clone_mut();
        try!(self.apply_mut(&mut mut_state));
        
        let state = PartState::from_mut_with_parents(mut_state,
                self.parents.clone(), self.meta.clone());
        if state.statesum() != self.statesum() { return Err(PatchOp::PatchApply); }
        Ok(state)
    }
    
    /// Apply this commit to a `MutPartState`. Unlike `apply()`, this does not
    /// verify the final statesum and does not use the metadata stored in this
    /// commit.
    pub fn apply_mut(&self, mut_state: &mut MutPartState<E>) -> Result<(), ElementOp> {
        for (id, ref change) in self.changes.iter() {
            match *change {
                &EltChange::Deletion => {
                    try!(mut_state.remove(*id));
                },
                &EltChange::Insertion(ref elt) => {
                    try!(mut_state.insert_with_id(*id, elt.clone()));
                }
                &EltChange::Replacement(ref elt) => {
                    try!(mut_state.replace_rc(*id, elt.clone()));
                }
                &EltChange::MovedOut(new_id) => {
                    try!(mut_state.remove(*id));
                    mut_state.set_move(*id, new_id);
                }
                &EltChange::Moved(new_id) => {
                    mut_state.set_move(*id, new_id);
                }
            }
        }
        Ok(())
    }
    
    /// Mutate the metadata in order to yield a new `statesum()` while
    /// otherwise not changing the state.
    /// 
    /// Requires the output of `State::mutate_meta()` to work correctly.
    /// Warning: the state used to do this must match this commit or this
    /// commit will be messed up!
    pub fn mutate_meta(&mut self, mutated: (u32, Sum)) {
        match &mut self.meta.meta {
            &mut MetaInternal::Meta1(ref mut m) => {
                m.number = mutated.0;
            },
        }
        self.statesum = mutated.1;
    }
    
    /// Get the state checksum
    pub fn statesum(&self) -> &Sum { &self.statesum }
    /// Get the parents. There must be at least one. The first is the primary,
    /// which can be patched by this commit.
    pub fn parents(&self) -> &Vec<Sum> { &self.parents }
    /// Get the first parent. This is the one the commit is applied against.
    pub fn first_parent(&self) -> &Sum { &self.parents[0] }
    /// Get the number of changes in the "patch"
    pub fn num_changes(&self) -> usize { self.changes.len() }
    /// Get an iterator over changes
    pub fn changes_iter(&self) -> hash_map::Iter<EltId, EltChange<E>> { self.changes.iter() }
    /// Access the commit's meta-data
    pub fn meta(&self) -> &CommitMeta { &self.meta }
    /// Write acces to the commit's meta-data
    pub fn meta_mut(&mut self) -> &mut CommitMeta { &mut self.meta }
}
