//! Pippin: commit structs and functionality

use std::collections::{HashSet, HashMap, hash_map};
use std::clone::Clone;
use std::rc::Rc;
use hashindexed::HashIndexed;

use detail::{ElementT};
use detail::{Sum, PartitionState, PartitionStateSumComparator};
use detail::readwrite::CommitReceiver;
use error::{Result, ReplayError, ElementOp};


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
    /// Add a commit to the end of the queue.
    pub fn push(&mut self, commit: Commit<E>) {
        self.commits.push(commit);
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
    changes: HashMap<u64, EltChange<E>>
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
    /// Get `Some(elt)` or `None`
    pub fn element(&self) -> Option<&Rc<E>> {
        match self {
            &EltChange::Deletion => None,
            &EltChange::Insertion(ref elt) => Some(elt),
            &EltChange::Replacement(ref elt) => Some(elt),
        }
    }
}

// —————  Commit operations  —————

impl<E: ElementT> Commit<E> {
    /// Create a commit from parts. It is suggested not to use this unless you
    /// are sure all sums are correct.
    /// 
    /// This also panics if parents.len() == 0.
    pub fn new(statesum: Sum, parents: Vec<Sum>, changes: HashMap<u64, EltChange<E>>) -> Commit<E> {
        assert!(parents.len() >= 1);
        Commit { statesum: statesum, parents: parents, changes: changes }
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
            match state.remove_rc(*id) {
                Ok(new_elt) => {
                    if new_elt == *old_elt {
                        /* no change */
                    } else {
                        changes.insert(*id, EltChange::replacement(new_elt));
                    }
                },
                Err(ElementOp{id: _,class: _}) => {
                    changes.insert(*id, EltChange::deletion());
                }
            }
        }
        for (id, new_elt) in state.into_map() {
            changes.insert(id, EltChange::insertion(new_elt));
        }
        
        if changes.is_empty() {
            None
        } else {
            Some(Commit {
                statesum: new_state.statesum().clone(),
                parents: vec![old_state.statesum().clone()],
                changes: changes
            })
        }
    }
    
    /// Apply this commit to a given state, thus updating the state.
    /// 
    /// Fails if the given state's initial state-sum is not equal to this
    /// commit's parent or if there are any errors in applying this patch.
    pub fn patch(&self, state: &mut PartitionState<E>) -> Result<()> {
        if state.statesum() != self.parent() {
            return ReplayError::err("cannot apply commit: state does not match parent checksum");
        }
        
        for (id, ref change) in self.changes.iter() {
            match *change {
                &EltChange::Deletion => {
                    try!(state.remove_rc(*id));
                },
                &EltChange::Insertion(ref elt) => {
                    try!(state.insert_rc(*id, elt.clone()));
                }
                &EltChange::Replacement(ref elt) => {
                    try!(state.replace_rc(*id, elt.clone()));
                }
            }
        }
        
        if state.statesum() != self.statesum() {
            return ReplayError::err("checksum failure of replayed commit");
        }
        Ok(())
    }
    
    /// Get the state checksum
    pub fn statesum(&self) -> &Sum { &self.statesum }
    /// Get the first parent's `Sum`
    pub fn parent(&self) -> &Sum { &self.parents[0] }
    /// Get the list of all parents' `Sum`s
    pub fn parents(&self) -> &Vec<Sum> { &self.parents }
    /// Get the number of changes in the "patch"
    pub fn num_changes(&self) -> usize { self.changes.len() }
    /// Get an iterator over changes
    pub fn changes_iter(&self) -> hash_map::Iter<u64, EltChange<E>> { self.changes.iter() }
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
    
    /// Insert an initial state, marked as a tip (pass by value or clone).
    pub fn add_state(&mut self, state: PartitionState<E>) {
        self.tips.insert(state.statesum().clone());
        self.states.insert(state);
    }
    
    /// Recreate all known states from a set of commits. On success, return a
    /// reference to self. Will fail if a commit applies to an unknown state or
    /// any checksum is incorrect.
    pub fn replay(&mut self, commits: CommitQueue<E>) -> Result<&Self> {
        for commit in commits.commits {
            let mut state = try!(self.states.get(&commit.parent())
                .ok_or(ReplayError::new("parent state of commit not found")))
                .clone_child();
            if self.states.contains(&commit.statesum) {
                // #0022: could verify that this state matches that derived from
                // the commit and warn if not.
                
                // Since the state is already known, it either is already
                // marked a tip or it has been unmarked. Do not set again!
                // However, we now know that the parent state isn't a tip, which
                // might not have been known before (if new state is a snapshot).
                self.tips.remove(&commit.parent());
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
            
            self.tips.remove(&commit.parent());
            self.tips.insert(commit.statesum);
        }
        Ok(self)
    }
}


#[test]
fn commit_creation_and_replay(){
    let part_id = 0;
    let mut commits = CommitQueue::<String>::new();
    
    let mut state_a = PartitionState::new(part_id);
    state_a.insert_elt(1, "one".to_string()).unwrap();
    state_a.insert_elt(2, "two".to_string()).unwrap();
    
    let mut state_b = state_a.clone_child();
    state_b.insert_elt(3, "three".to_string()).unwrap();
    state_b.insert_elt(4, "four".to_string()).unwrap();
    state_b.insert_elt(5, "five".to_string()).unwrap();
    commits.push(Commit::from_diff(&state_a, &state_b).unwrap());
    
    let mut state_c = state_b.clone_child();
    state_c.insert_elt(6, "six".to_string()).unwrap();
    state_c.insert_elt(7, "seven".to_string()).unwrap();
    state_c.remove_elt(4).unwrap();
    state_c.replace_elt(3, "half six".to_string()).unwrap();
    commits.push(Commit::from_diff(&state_b, &state_c).unwrap());
    
    let mut state_d = state_c.clone_child();
    state_d.insert_elt(8, "eight".to_string()).unwrap();
    state_d.insert_elt(4, "half eight".to_string()).unwrap();
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
