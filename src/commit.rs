/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Pippin: commit structs and functionality

use std::collections::{HashMap, hash_map};
use std::clone::Clone;
use std::rc::Rc;
use std::u32;
use std::cmp::max;
use std::ops::BitOr;

use chrono::{DateTime, NaiveDateTime, UTC};

use state::{PartState, MutPartState, MutStateT};
use elt::{ElementT, EltId};
use sum::Sum;
use error::{Result, ElementOp, OtherError};


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

const FLAG_RECLASSIFY_BIT: u16 = 0b10;
const FLAG_RECLASSIFY_MASK: u16 = 0b11;
const FLAG_ESSENTIAL: u16 = 0b01010101_01010101;
const FLAG_UNKNOWN: u16 = 0b11111111_11111100;

/// Abstraction around metadata flags.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct MetaFlags {
    flags: u16,
}
impl MetaFlags {
    /// Get extension flags as a u16. This isn't intended to allow direct
    /// manipulation, only to allow the bit-field to be saved.
    pub fn raw(self) -> u16 {
        self.flags
    }
    /// Create from a raw u16.
    pub fn from_raw(flags: u16) -> MetaFlags {
        MetaFlags { flags: flags }
    }
    
    /// Get status of "reclassify" flag. If true, reclassification is needed
    /// and will be done in a maintenance cycle.
    pub fn flag_reclassify(self) -> bool {
        (self.flags & FLAG_RECLASSIFY_BIT) != 0
    }
    /// Set "reclassify" flag.
    pub fn set_flag_reclassify(&mut self, state: bool) {
        if state {
            // set only flag not 'essential' bit since this feature is not essential to correct reading
            self.flags |= FLAG_RECLASSIFY_BIT;
        } else {
            // mask with inverse of "reclassify" bits mask
            self.flags &= !FLAG_RECLASSIFY_MASK;
        }
    }
    /// True if the essential bit of an unknown flag is set
    pub fn unknown_essential(self) -> bool {
        let mask = FLAG_ESSENTIAL & FLAG_UNKNOWN;
        (self.flags & mask) != 0
    }
    /// Create, with no flags set
    pub fn zero() -> MetaFlags {
        MetaFlags { flags: 0 }
    }
}

impl BitOr<MetaFlags> for MetaFlags {
    type Output = MetaFlags;
    fn bitor(self, rhs: MetaFlags) -> MetaFlags {
        MetaFlags { flags: self.flags | rhs.flags }
    }
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
    /// Extension flags. These are inherited verbatim, so stored in this format.
    ext_flags: MetaFlags,
    /// User-provided extra metadata
    extra: ExtraMeta,
}

/// Partial version of metadata (used by some functions on CommitMeta).
#[derive(Debug, PartialEq, Clone)]
pub struct CommitMetaPartial {
    parent: (Sum, CommitMeta),
    ext_flags: MetaFlags,
}


impl CommitMeta {
    /// Create from parent(s)' data and a `MakeCommitMeta` trait.
    pub fn new_parents(parents: Vec<(&Sum, &CommitMeta)>, mcm: &MakeCommitMeta) -> Self {
        let number = parents.iter().fold(0, |prev, &p| max(prev, p.1.next_number()));
        let ext_flags = parents.iter().fold(MetaFlags::zero(), |prev, &p| prev | p.1.ext_flags());
        CommitMeta {
            number: number,
            timestamp: mcm.make_commit_timestamp(),
            ext_flags: ext_flags,
            extra: mcm.make_commit_extra(number, parents),
        }
    }
    /// Create, explicitly providing all fields.
    pub fn new_explicit(number: u32, timestamp: i64, ext_flags: MetaFlags,
            _ext_data: Vec<u8>, extra: ExtraMeta) -> Result<Self, OtherError>
    {
        if (ext_flags.unknown_essential()) {
            return Err(OtherError::new("found essential unknown commit meta flag"));
        }
        // ignore ext_data because we can and don't currently store anything there
        Ok(CommitMeta { number: number, timestamp: timestamp, ext_flags: ext_flags, extra: extra })
    }
    /// Create a partial new version from a single parent.
    /// 
    /// This is for use with `from_partial()`.
    pub fn new_partial(par_sum: Sum, par_meta: CommitMeta) -> CommitMetaPartial {
        let ext_flags = par_meta.ext_flags();   // copied to allow modification
        CommitMetaPartial {
            parent: (par_sum, par_meta),
            ext_flags: ext_flags,
        }
    }
    /// Create, from a partial version (assumes a single parent commit).
    /// 
    /// This sets timestamp and user data (extra meta).
    pub fn from_partial(partial: CommitMetaPartial, mcm: &MakeCommitMeta) -> CommitMeta {
        let number = partial.parent.1.next_number();
        let parent = (&partial.parent.0, &partial.parent.1);
        
        CommitMeta {
            number: number,
            timestamp: mcm.make_commit_timestamp(),
            ext_flags: partial.ext_flags,
            extra: mcm.make_commit_extra(number, vec![parent]),
        }
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
    
    /// Get extension flags
    pub fn ext_flags(&self) -> MetaFlags {
        self.ext_flags
    }
    
    /// Get the commit's extra data.
    pub fn extra(&self) -> &ExtraMeta {
        &self.extra
    }
}

impl CommitMetaPartial {
    /// Get the parent's metadata
    pub fn parent_meta(&self) -> &CommitMeta {
        &self.parent.1
    }
    
    /// Get the commit's number
    pub fn number(&self) -> u32 {
        self.parent.1.next_number()
    }
    
    /// Get extension flags
    pub fn ext_flags(&self) -> MetaFlags {
        self.ext_flags
    }
    
    /// Get extension flags, mutably
    pub fn ext_flags_mut(&mut self) -> &mut MetaFlags {
        &mut self.ext_flags
    }
}


/// Interface used to customise commit metadata
pub trait MakeCommitMeta {
    /// Controls creation of commit timestamps. The default implementation simply wraps
    /// `CommitMeta::timestamp_now()`.
    /// 
    /// The library itself does not depend on the value of these timestamps, it simply provides
    /// them as a convenience.
    fn make_commit_timestamp(&self) -> i64 {
        CommitMeta::timestamp_now()
    }
    
    /// Make an extra-metadata item. The default implementation simply
    /// returns `ExtraMeta::None`.
    /// 
    /// Arguments: this commit's number, and the commit identifier and metadata for each parent
    /// commit.
    /// The commit number and the sum of each parent commit is passed.
    fn make_commit_extra(&self, _number: u32, _parents: Vec<(&Sum, &CommitMeta)>) -> ExtraMeta {
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
    /// State sum (ID) of parent states. There must be at least one. The first
    /// is the *primary parent* and is what the *changes* are relative
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
    MoveOut(EltId),
    /// Same as `MoveOut` except that the element has already been removed from the partition
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
            true => EltChange::MoveOut(new_id),
            false => EltChange::Moved(new_id),
        }
    }
    /// Get `Some(elt)` if an element is contained, `None` otherwise
    pub fn element(&self) -> Option<&Rc<E>> {
        match self {
            &EltChange::Deletion => None,
            &EltChange::Insertion(ref elt) => Some(elt),
            &EltChange::Replacement(ref elt) => Some(elt),
            &EltChange::MoveOut(_) => None,
            &EltChange::Moved(_) => None,
        }
    }
    /// Get `Some(new_id)` if this is a "moved" change, else None
    pub fn moved_id(&self) -> Option<EltId> {
        match self {
            &EltChange::Deletion => None,
            &EltChange::Insertion(_) => None,
            &EltChange::Replacement(_) => None,
            &EltChange::MoveOut(id) => Some(id),
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
        // #0019: is using `collect()` for a HashMap efficient? Better to add a "clone_map" function to new_state?
        let mut elt_map: HashMap<_,_> = new_state.elts_iter().collect();
        let mut changes = HashMap::new();
        for (id, old_elt) in old_state.elts_iter() {
            if let Some(new_elt) = elt_map.remove(&id) {
                // #0019: should we compare sums? Would this be faster?
                if new_elt == old_elt {
                    /* no change */
                } else {
                    changes.insert(id, EltChange::replacement(new_elt.clone()));
                }
            } else {
                // not in new state: has been deleted
                changes.insert(id, EltChange::deletion());
            }
        }
        // #0019: is using `collect()` for a HashMap efficient? Better to add a "clone_map" function to new_state?
        let mut moved_map: HashMap<_,_> = new_state.moved_iter().collect();
        for (id, new_id) in old_state.moved_iter() {
            if let Some(new_id2) = moved_map.remove(&id) {
                if new_id == new_id2 {
                    /* no change */
                } else {
                    changes.insert(id, EltChange::moved(new_id2, false));
                }
            } else {
                // we seem to have forgotten that an element was moved
                // TODO: why? Should we track this so patches can make states *forget*?
                // TODO: should we warn about it?
            }
        }
        for (id, new_elt) in elt_map {
            changes.insert(id, EltChange::insertion(new_elt.clone()));
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
                    mut_state.remove(*id)?;
                },
                &EltChange::Insertion(ref elt) => {
                    mut_state.insert_with_id(*id, elt.clone())?;
                }
                &EltChange::Replacement(ref elt) => {
                    mut_state.replace_rc(*id, elt.clone())?;
                }
                &EltChange::MoveOut(new_id) => {
                    mut_state.remove(*id)?;
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
    pub fn parents(&self) -> &[Sum] { &self.parents }
    /// Get the first parent. This is the one the commit is applied against.
    /// 
    /// This is identical to calling `commit.parents()[0]`, but clarifies that
    /// the first parent is special and always present.
    pub fn first_parent(&self) -> &Sum { &self.parents[0] }
    /// Get the number of changes in the "patch"
    pub fn num_changes(&self) -> usize { self.changes.len() }
    /// Get an iterator over changes
    pub fn changes_iter(&self) -> hash_map::Iter<EltId, EltChange<E>> { self.changes.iter() }
    /// Get a specific change, if this element was changed
    pub fn change(&self, id: EltId) -> Option<&EltChange<E>> {
        self.changes.get(&id)
    }
    /// Access the commit's meta-data
    pub fn meta(&self) -> &CommitMeta { &self.meta }
    /// Write acces to the commit's meta-data
    pub fn meta_mut(&mut self) -> &mut CommitMeta { &mut self.meta }
}
