/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Pippin: commit structs and functionality

use std::collections::{HashSet, HashMap, hash_map};
use std::clone::Clone;
use std::rc::Rc;
use std::u32;

use hashindexed::HashIndexed;
use chrono::{DateTime, NaiveDateTime, UTC};

use detail::states::PartitionStateSumComparator;
use detail::readwrite::CommitReceiver;
use partition::{PartitionState, State};
use {ElementT, EltId, Sum};
use error::{Result, ReplayError, PatchOp};


/// Extra data about a commit
#[derive(Eq, PartialEq, Clone, Debug)]
pub struct CommitMeta {
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
    /// 
    /// Currently this is either unicode text or nothing but there is a
    /// possibility of allowing other types (probably by replacing `Option`
    /// with a custom enum).
    pub extra: Option<String>,
}
impl CommitMeta {
    /// Create an instance applicable to a new empty partition.
    /// 
    /// Assigns a timestamp as of *now* (via `Self::timestamp_now()`).
    pub fn new_empty() -> CommitMeta {
        CommitMeta {
            number: 0,
            timestamp: Self::timestamp_now(),
            extra: None,
        }
    }
    /// Create an instance. Set number to `prev_num` + 1 (but without
    /// wrapping) and extra to `extra`.
    /// 
    /// Assigns a timestamp as of *now* (via `Self::timestamp_now()`).
    pub fn new_from(prev_num: u32, extra: Option<String>) -> CommitMeta {
        CommitMeta {
            number: if prev_num < u32::MAX { prev_num + 1 } else { prev_num },
            timestamp: Self::timestamp_now(),
            extra: extra,
        }
    }
    /// Convert the internal timestamp to a `DateTime`.
    pub fn date_time(&self) -> DateTime<UTC> {
        DateTime::<UTC>::from_utc(
                NaiveDateTime::from_timestamp(self.timestamp, 0),
                UTC)
    }
    /// Create a timestamp representing this moment.
    pub fn timestamp_now() -> i64 {
        UTC::now().timestamp()
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
    /// Get the number of items in the queue
    pub fn len(&self) -> usize {
        self.commits.len()
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
    pub fn new(statesum: Sum, parents: Vec<Sum>,
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
    pub fn from_diff(old_state: &PartitionState<E>, new_state: &PartitionState<E>) -> Option<Commit<E>> {
        let mut state = new_state.clone_exact();
        let mut changes = HashMap::new();
        for (id, old_elt) in old_state.map() {
            match state.remove(*id) {
                Ok(new_elt) => {
                    if new_elt == *old_elt {
                        /* no change */
                    } else {
                        changes.insert(*id, EltChange::replacement(new_elt));
                    }
                },
                Err(_) => {
                    // we assume the failure is because the element does not exist
                    changes.insert(*id, EltChange::deletion());
                }
            }
        }
        let (elt_map, mut moved_map) = state.into_maps();
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
                //FIXME: new_state probably does not have correct statesum, but what do we take for granted? Recalculate?
                statesum: new_state.statesum().clone(),
                parents: vec![old_state.statesum().clone()],
                changes: changes,
                meta: CommitMeta::new_from(old_state.meta().number, None),
            })
        }
    }
    
    /// Apply this commit to a given state, thus updating the state.
    /// 
    /// Fails if the given state's initial state-sum is not equal to this
    /// commit's parent or if there are any errors in applying this patch.
    pub fn patch(&self, state: &mut PartitionState<E>) -> Result<(), PatchOp> {
        if *state.statesum() != self.parents[0] { return Err(PatchOp::WrongParent); }
        
        // #0018: we cast ElementOp errors to PatchOp via a `From`
        // implementation. Ideally we'd use a `try` block and only map here
        // since in other cases we needn't map.
        for (id, ref change) in self.changes.iter() {
            match *change {
                &EltChange::Deletion => {
                    try!(state.remove(*id));
                },
                &EltChange::Insertion(ref elt) => {
                    try!(state.insert_with_id(*id, elt.clone()));
                }
                &EltChange::Replacement(ref elt) => {
                    try!(state.replace_rc(*id, elt.clone()));
                }
                &EltChange::MovedOut(new_id) => {
                    try!(state.remove(*id));
                    state.set_move(*id, new_id);
                }
                &EltChange::Moved(new_id) => {
                    state.set_move(*id, new_id);
                }
            }
        }
        
        if state.statesum() != self.statesum() { return Err(PatchOp::PatchApply); }
        Ok(())
    }
    
    /// Get the state checksum
    pub fn statesum(&self) -> &Sum { &self.statesum }
    /// Get the parents. There must be at least one. The first is the primary,
    /// which can be patched by this commit.
    pub fn parents(&self) -> &Vec<Sum> { &self.parents }
    /// Get the number of changes in the "patch"
    pub fn num_changes(&self) -> usize { self.changes.len() }
    /// Get an iterator over changes
    pub fn changes_iter(&self) -> hash_map::Iter<EltId, EltChange<E>> { self.changes.iter() }
    /// Access the commit's meta-data
    pub fn meta(&self) -> &CommitMeta { &self.meta }
    /// Write acces to the commit's meta-data
    pub fn meta_mut(&mut self) -> &mut CommitMeta { &mut self.meta }
}


// —————  log replay  —————

pub type StatesSet<E> = HashIndexed<PartitionState<E>, Sum, PartitionStateSumComparator>;

/// Struct holding data used during log replay.
///
/// This stores *all* recreated states since it does not know which may be used
/// as parents of future commits. API currently only allows access to the tip,
/// but could be modified.
pub struct LogReplay<'a, E: ElementT+'a> {
    states: &'a mut StatesSet<E>,
    tips: &'a mut HashSet<Sum>
}

impl<'a, E: ElementT> LogReplay<'a, E> {
    /// Create the structure, binding to two sets. These may be empty; in this
    /// case call `add_state()` to add an initial state.
    pub fn from_sets(states: &'a mut StatesSet<E>, tips: &'a mut HashSet<Sum>) -> LogReplay<'a, E> {
        LogReplay { states: states, tips: tips }
    }
    
    /// Recreate all known states from a set of commits. On success, return the
    /// number of edits (insertions, deletions or replacements).
    /// 
    /// Will fail if a commit applies to an unknown state or
    /// any checksum is incorrect.
    pub fn replay(&mut self, commits: CommitQueue<E>) -> Result<usize> {
        let mut edits = 0;
        for commit in commits.commits {
            let mut state = try!(self.states.get(&commit.parents()[0])
                .ok_or(ReplayError::new("parent state of commit not found")))
                .clone_child();
            if self.states.contains(&commit.statesum) {
                // #0022: could verify that this state matches that derived from
                // the commit and warn if not.
                
                // Since the state is already known, it either is already
                // marked a tip or it has been unmarked. Do not set again!
                // However, we now know that the parent states aren't tips, which
                // might not have been known before (if new state is a snapshot).
                for parent in commit.parents() {
                    self.tips.remove(parent);
                }
                continue;
            }
            
            try!(commit.patch(&mut state));
            
            let has_existing = if let Some(existing) = self.states.get(&state.statesum()) {
                if *existing != state {
                    // Collision. We can't do much in this case, so just warn about it.
                    // #0017: warn about collision
                }
                true
            } else { false };
            if !has_existing {
                self.states.insert(state);
            }
            
            for parent in commit.parents() {
                self.tips.remove(parent);
            }
            self.tips.insert(commit.statesum);
            edits += commit.changes.len();
        }
        Ok(edits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::{ElementT, PartitionState};
    use hashindexed::HashIndexed;
    use std::collections::HashSet;
    
    impl<E: ElementT> CommitQueue<E> {
        /// Add a commit to the end of the queue.
        pub fn push(&mut self, commit: Commit<E>) {
            self.commits.push(commit);
        }
    }
    
    impl<'a, E: ElementT> LogReplay<'a, E> {
        /// Insert an initial state, marked as a tip (pass by value or clone).
        pub fn add_state(&mut self, state: PartitionState<E>) {
            self.tips.insert(state.statesum().clone());
            self.states.insert(state);
        }
    }
    
    
    #[test]
    fn commit_creation_and_replay(){
        use {PartId, State};
        use std::rc::Rc;
        
        let p = PartId::from_num(1);
        let mut commits = CommitQueue::<String>::new();
        
        let insert = |state: &mut PartitionState<_>, num, string: &str| -> Result<_, _> {
            state.insert_with_id(p.elt_id(num), Rc::new(string.to_string()))
        };
        
        let mut state_a = PartitionState::new(p);
        insert(&mut state_a, 1, "one").unwrap();
        insert(&mut state_a, 2, "two").unwrap();
        
        let mut state_b = state_a.clone_child();
        insert(&mut state_b, 3, "three").unwrap();
        insert(&mut state_b, 4, "four").unwrap();
        insert(&mut state_b, 5, "five").unwrap();
        commits.push(Commit::from_diff(&state_a, &state_b).unwrap());
        
        let mut state_c = state_b.clone_child();
        insert(&mut state_c, 6, "six").unwrap();
        insert(&mut state_c, 7, "seven").unwrap();
        state_c.remove(p.elt_id(4)).unwrap();
        state_c.replace(p.elt_id(3), "half six".to_string()).unwrap();
        commits.push(Commit::from_diff(&state_b, &state_c).unwrap());
        
        let mut state_d = state_c.clone_child();
        insert(&mut state_d, 8, "eight").unwrap();
        insert(&mut state_d, 4, "half eight").unwrap();
        commits.push(Commit::from_diff(&state_c, &state_d).unwrap());
        
        let (mut states, mut tips) = (HashIndexed::new(), HashSet::new());
        {
            let mut replayer = LogReplay::from_sets(&mut states, &mut tips);
            replayer.add_state(state_a);
            replayer.replay(commits).unwrap();
        }
        assert_eq!(tips.len(), 1);
        let tip_sum = tips.iter().next().unwrap();
        let replayed_state = states.remove(&tip_sum).unwrap();
        assert_eq!(replayed_state, state_d);
    }
}
