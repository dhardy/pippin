//! Pippin: commit structs and functionality

use std::collections::{HashSet, HashMap};
use std::clone::Clone;

use detail::{Sum, Element, RepoState};
use detail::readwrite::CommitReceiver;
use ::{Result, Error};
use hashindexed::{HashIndexed, KeyComparator};


/// Holds a set of commits, ordered by insertion order.
/// This is only really needed for readwrite::read_log().
struct CommitQueue {
    // These must be ordered. We only ever access by iteration so a `Vec` is
    // fine.
    commits: Vec<Commit>
}

impl CommitQueue {
    /// Create an empty CommitQueue.
    pub fn new() -> CommitQueue {
        CommitQueue { commits: Vec::new() }
    }
    /// Add a commit to the end of the queue.
    pub fn push(&mut self, commit: Commit) {
        self.commits.push(commit);
    }
}

impl CommitReceiver for CommitQueue {
    /// Implement function required by readwrite::read_log().
    fn receive(&mut self, statesum: Sum, parent: Sum,
        changes: HashMap<u64, EltChange>) -> bool {
        
        self.commits.push(
            Commit { statesum: statesum, parent: parent, changes: changes });
            //TODO: what should we do when commit statesums clash?
            //
            // Possible exploit: someone forks the repo, creates a new state
            // decending from an ancestor of commit X but with checksum
            // collision against X, pushes it back to the local repo and thus
            // rewrites history. Okay, this involves being able to forge SHA256
            // hash collisions *and* get your target to pull your changes.
            // 
            // More likely case: two commits are made which reach the same
            // state and thus have the same sum. One gets ignored. Maybe okay?
            //
            // Possible bug: a commit reverts to the previous state, thus its
            // sum collides with that of an ancestor commit. This could be
            // problematic!
        
        true    // continue reading to EOF
    }
}


/// A commit: a set of changes
///
/// Note: "eq" is implemented such that two commits are equal when their
/// statesum is equal. See note on PartialEq implementation for ramifications.
struct Commit {
    /// Expected resultant state sum; doubles as an ID
    statesum: Sum,
    /// State sum (ID) of parent commit/snapshot
    parent: Sum,
    /// Per-element changes
    changes: HashMap<u64, EltChange>
}

/// Per-element changes
pub enum EltChange {
    /// Element was deleted
    Deletion,
    /// Element was added (full data)
    Insertion(Element),
    /// Element was replaced (full data)
    Replacement(Element),
    //TODO: patches (?)
}
impl EltChange {
    pub fn insertion(elt: Element) -> EltChange {
        EltChange::Insertion(elt)
    }
    pub fn replacement(elt: Element) -> EltChange {
        EltChange::Replacement(elt)
    }
    pub fn deletion() -> EltChange {
        EltChange::Deletion
    }
}

// —————  patch creation from differences  —————

impl Commit {
    /// Create a commit from an old state and a new state.
    /// 
    /// This is one of two ways to create a commit; the other would be to track
    /// changes to a state (possibly the latter is the more sensible approach
    /// for most applications).
    pub fn from_diff(old_state: &RepoState, new_state: &RepoState) -> Commit {
        let mut state = new_state.clone();
        let mut changes = HashMap::new();
        for (id, old_elt) in old_state.map() {
            match state.remove_elt(*id) {
                Ok(new_elt) => {
                    if new_elt == *old_elt {
                        /* no change */
                    } else {
                        changes.insert(*id, EltChange::replacement(new_elt));
                    }
                },
                Err(Error::NoEltFound(_)) => {
                    changes.insert(*id, EltChange::deletion());
                },
                Err(_) => panic!("should be impossible") /*TODO refactor or deal with this properly*/
            }
        }
        for (id, new_elt) in state.map() /*TODO: into iter*/ {
            changes.insert(*id, EltChange::insertion(new_elt.clone() /*TODO move not clone*/));
        }
        
        Commit {
            statesum: new_state.statesum(),
            parent: old_state.statesum(),
            changes: changes
        }
    }
}


// —————  log replay  —————

struct SumComparator;
impl KeyComparator<RepoState, Sum> for SumComparator {
    fn extract_key(value: &RepoState) -> &Sum {
        value.statesum_ref()
    }
}

/// Struct holding data used during log replay.
///
/// This stores *all* recreated states since it does not know which may be used
/// as parents of future commits. API currently only allows access to the tip,
/// but could be modified.
struct LogReplay {
    states: HashIndexed<RepoState, Sum, SumComparator>,
    tips: HashSet<Sum>
}

impl LogReplay {
    /// Create the structure from an initial state and sum
    pub fn from_initial(state: RepoState) -> LogReplay {
        let mut states = HashIndexed::new();
        let sum = state.statesum();
        states.insert(state);
        let mut tips = HashSet::new();
        tips.insert(sum);
        LogReplay { states: states, tips: tips }
    }
    
    /// Recreate all known states from a set of commits. On success, return a
    /// reference to self. Will fail if a commit applies to an unknown state or
    /// any checksum is incorrect.
    pub fn replay(&mut self, commits: CommitQueue) -> Result<&Self> {
        for commit in commits.commits {
            let mut state = try!(self.states.get(&commit.parent)
                .ok_or(Error::replay("parent state of commit not found")))
                .clone();
            
            for (id,change) in commit.changes {
                match change {
                    EltChange::Deletion => {
                        try!(state.remove_elt(id));
                    },
                    EltChange::Insertion(elt) => {
                        try!(state.insert_elt(id, elt));
                    }
                    EltChange::Replacement(elt) => {
                        try!(state.replace_elt(id, elt));
                    }
                }
            }
            
            if state.statesum() != commit.statesum {
                return Err(Error::replay("checksum failure of replayed commit"));
            }
            //TODO: what if there's a collision now??
            self.states.insert(state);
            
            self.tips.insert(commit.statesum);
            self.tips.remove(&commit.parent);
        }
        Ok(self)
    }
    
    /// Merge all latest states into a single tip.
    pub fn merge(&mut self) -> Result<&Self> {
        //TODO: write and test
        Ok(self)
    }
    
    /// Return the latest state, if there is a single latest state; otherwise
    /// fail. You should probably call merge() first to make sure there is only
    /// a single latest state.
    pub fn tip(&self) -> Result<&RepoState> {
        let tip = try!(self.tip_sum());
        Ok(self.states.get(&tip).expect("tip should point to a state"))
    }
    
    /// As tip(), but consume self and return ownership of the state.
    pub fn into_tip(mut self) -> Result<RepoState> {
        let tip = try!(self.tip_sum());
        Ok(self.states.remove(&tip).expect("tip should point to a state"))
    }
    
    fn tip_sum(&self) -> Result<Sum> {
        if self.tips.len() > 1 {
            return Err(Error::replay("no single latest state (merge required)"));
        }
        for tip in &self.tips {
            return Ok(*tip);
        }
        panic!("There should be at least one tip!")
    }
}


#[test]
fn commit_creation_and_replay(){
    let mut state = RepoState::new();
    let mut commits = CommitQueue::new();
    
    state.insert_elt(1, Element::from_str("one")).unwrap();
    state.insert_elt(2, Element::from_str("two")).unwrap();
    let state_a = state.clone();
    
    state.insert_elt(3, Element::from_str("three")).unwrap();
    state.insert_elt(4, Element::from_str("four")).unwrap();
    state.insert_elt(5, Element::from_str("five")).unwrap();
    let state_b = state.clone();
    commits.push(Commit::from_diff(&state_a, &state_b));
    
    state.insert_elt(6, Element::from_str("six")).unwrap();
    state.insert_elt(7, Element::from_str("seven")).unwrap();
    state.remove_elt(4).unwrap();
    state.replace_elt(3, Element::from_str("half six")).unwrap();
    let state_c = state.clone();
    commits.push(Commit::from_diff(&state_b, &state_c));
    
    state.insert_elt(8, Element::from_str("eight")).unwrap();
    state.insert_elt(4, Element::from_str("half eight")).unwrap();
    let state_d = state.clone();
    commits.push(Commit::from_diff(&state_c, &state_d));
    
    let mut replayer = LogReplay::from_initial(state_a);
    replayer.replay(commits).unwrap();
    let replayed_state = replayer.into_tip().unwrap();
    assert_eq!(replayed_state, state_d);
}
