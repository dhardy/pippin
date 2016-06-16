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

use {PartState, MutPartState, MutState};
use {ElementT, EltId, Sum};
use error::{Result, ElementOp};


/// The type of the user-specified *extra* metadata field. This allows users
/// to store extra information about commits (e.g. author, place, comment).
/// 
/// Currently the only supported non-empty type is UTF-8 text (designated XMTT
/// in files), but the file format and API allows for future extensions.
#[derive(Clone, PartialEq, Debug)]
pub enum ExtraMeta {
    /// No extra metadata
    None,
    /// Extra metadata as a simple text field
    Text(String),
}

/// Metadata is attached to every commit. The following is included by the
/// library:
/// 
/// *   The `number` of the commit (roughly, the length of the longest sequence
///     of ancestors leading back to the initial commit)
/// *   A time-stamp (usually the UTC time of creation)
/// 
/// Additionally, users may attach information via the `ExtraMeta` struct.
#[derive(Debug, PartialEq, Clone)]
pub struct CommitMeta {
    /// Commit number. First (real) commit has number 1, each subsequent commit
    /// has max-parent-number + 1. Can be used to identify commits but is not
    /// necessarily unique.
    number: u32,
    /// Time of commit creation
    /// 
    /// This is a UNIX time-stamp: the number of non-leap seconds since January
    /// 1, 1970 0:00:00 UTC. See `date_time()` or use
    /// `chrono::NaiveDateTime::from_timestamp` directly.
    /// 
    /// In rare cases this may be zero. 
    timestamp: i64,
    /// User-provided extra metadata
    extra: ExtraMeta,
}

impl CommitMeta {
    /// Create, with a specified number and an optional `MakeMeta` trait.
    pub fn new_num_mm(number: u32, mm: Option<&MakeMeta>) -> Self {
        let timestamp = mm.map_or_else(|| Self::timestamp_now(), |mm| mm.make_timestamp());
        CommitMeta {
            number: number,
            timestamp: timestamp,
            extra: mm.map_or(ExtraMeta::None, |mm| mm.make_extrameta()),
        }
    }
    /// Create from parent(s)' metadata and an optional `MakeMeta` trait.
    pub fn new_par_mm(parents: Vec<&CommitMeta>, mm: Option<&MakeMeta>) -> Self {
        let number = parents.iter().fold(0, |prev, &m| max(prev, m.next_number()));
        let timestamp = mm.map_or_else(|| Self::timestamp_now(), |mm| mm.make_timestamp());
        CommitMeta {
            number: number,
            timestamp: timestamp,
            extra: mm.map_or(ExtraMeta::None, |mm| mm.make_extrameta()),
        }
    }
    /// Create, explicitly providing all fields.
    pub fn new_explicit(number: u32, timestamp: i64, extra: ExtraMeta) -> Self {
        CommitMeta { number: number, timestamp: timestamp, extra: extra }
    }
    
    /// Utility method to create a timestamp representing this moment.
    /// 
    /// Code is `UTC::now().timestamp()`, using `chrono::UTC`.
    pub fn timestamp_now() -> i64 {
        UTC::now().timestamp()
    }
    
    /// Get the commit's timestamp
    pub fn timestamp(&self) -> i64 {
        self.timestamp
    }
    /// Convert the internal timestamp to a `chrono::DateTime`.
    pub fn date_time(&self) -> DateTime<UTC> {
        DateTime::<UTC>::from_utc(
                NaiveDateTime::from_timestamp(self.timestamp, 0),
                UTC)
    }
    
    /// Get the commit's number
    pub fn number(&self) -> u32 {
        self.number
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
        self.number = n;
    }
    
    /// Get the commit's extra data.
    pub fn extra(&self) -> &ExtraMeta {
        &self.extra
    }
}

/// Interface used to assign user-specific metadata.
/// 
/// This mostly exists to allow users to specify the contents of the "extra
/// metadata" field, which otherwise remains empty.
/// 
/// It is also possible to change the way timestamps are assigned with this
/// trait, but it is up to the user to avoid causing confusion when this
/// happens. Timestamps are not acutally used for anything by the library.
pub trait MakeMeta {
    /// Make a timestamp for a commit (usually the time now). The default
    /// implementation simply wraps `CommitMeta::timestamp_now()`.
    fn make_timestamp(&self) -> i64 {
        CommitMeta::timestamp_now()
    }
    
    /// Make an extra-metadata item. The default implementation simply
    /// returns `ExtraMeta::None`.
    fn make_extrameta(&self) -> ExtraMeta {
        ExtraMeta::None
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
    pub fn from_diff(old_state: &PartState<E>, new_state: &PartState<E>)
            -> Option<Commit<E>>
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
            Some(Commit {
                statesum: new_state.statesum().clone(),
                parents: vec![old_state.statesum().clone()],
                changes: changes,
                meta: new_state.meta().clone(),
            })
        }
    }
    
    /// Apply this commit to a `MutPartState`. This does not verify the final
    /// statesum and does not use the metadata stored in this commit.
    /// 
    /// You should probably use
    /// `PartState::from_state_commit(&par_state, &commit)` instead of using
    /// this method directly.
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
        self.meta.number = mutated.0;
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
