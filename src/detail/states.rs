//! Pippin: support for dealing with log replay, commit creation, etc.

/*!
In memory representation:

We create a `RepoState` to track each state of the repo we are interested in.
This is essentially a map from element identifiers to reference-counted
elements.

TODO: do we look for all states without successors and try to merge if we have
multiple or what?

During log-replay we keep track of each state encountered, using the state sum
as the key. This is for doing merges.

On any change, a new element is inserted (we treat elements as copy-on-write),
but we keep track of the state of the last recorded commit as well as the
latest in-memory state. On writing a commit, we TODO.

Commit creation: go over all differences between two *states* or maintain a
list of *changes*? Maybe doesn't matter since this is a special case and not
very complex.

TODO: when partitioning is introduced, some of this will change.
*/

use std::collections::{HashSet, HashMap};
use std::collections::hash_map::{Keys};
use std::clone::Clone;
use hashindexed::{HashIndexed, KeyComparator};

use detail::{Sum, Element};
use detail::readwrite::CommitReceiver;
use ::{Result, Error};

/// State of the repository (see module doc)
#[derive(PartialEq,Eq,Debug)]
pub struct RepoState {
    statesum: Sum,
    elts: HashMap<u64, Element>
}

impl RepoState {
    /// Create a new state, with no elements
    pub fn new() -> RepoState {
        RepoState { statesum: Sum::zero(), elts: HashMap::new() }
    }
    /// Create from a map of elements
    pub fn from_hash_map(map: HashMap<u64, Element>, statesum: Sum) -> RepoState {
        RepoState { statesum: statesum, elts: map }
    }
    
    /// Get the state sum
    pub fn statesum(&self) -> Sum {
        self.statesum
    }
    
    /// Get access to the map holding elements
    pub fn map(&self) -> &HashMap<u64, Element> {
        &self.elts
    }
    /// Get a reference to some element.
    /// 
    /// Note that elements can't be modified directly but must instead be
    /// replaced, hence there is no version of this function returning a
    /// mutable reference.
    pub fn get_elt(&self, id: u64) -> Option<&Element> {
        self.elts.get(&id)
    }
    /// Get the number of elements
    pub fn num_elts(&self) -> usize {
        self.elts.len()
    }
    /// Is an element with a given key present?
    pub fn has_elt(&self, id: u64) -> bool {
        self.elts.contains_key(&id)
    }
    /// Get the element keys
    pub fn elt_ids(&self) -> Keys<u64, Element> {
        self.elts.keys()
    }
    
    /// Insert an element and return (), unless the id is already used in
    /// which case the function stops with an error.
    pub fn insert_elt(&mut self, id: u64, elt: Element) -> Result<()> {
        if self.elts.contains_key(&id) { return Err(Error::arg("insertion conflicts with an existing element")); }
        self.statesum = self.statesum ^ elt.sum();
        self.elts.insert(id, elt);
        Ok(())
    }
    /// Replace an existing element and return the replaced element, unless the
    /// id is not already used in which case the function stops with an error.
    pub fn replace_elt(&mut self, id: u64, elt: Element) -> Result<Element> {
        self.statesum = self.statesum ^ elt.sum();
        match self.elts.insert(id, elt) {
            None => Err(Error::no_elt("replacement failed: no existing element")),
            Some(removed) => {
                self.statesum = self.statesum ^ removed.sum();
                Ok(removed)
            }
        }
    }
    /// Remove an element, returning it.
    pub fn remove_elt(&mut self, id: u64) -> Result<Element> {
        match self.elts.remove(&id) {
            None => Err(Error::no_elt("deletion failed: no element")),
            Some(removed) => {
                self.statesum = self.statesum ^ removed.sum();
                Ok(removed)
            }
        }
    }
}

impl Clone for RepoState {
    /// Clone the state. Elements are considered Copy-On-Write so cloning the
    /// state is not particularly expensive.
    fn clone(&self) -> Self {
        RepoState { statesum: self.statesum, elts: self.elts.clone() }
    }
}

struct CommitSumComparator;
impl KeyComparator<Commit, Sum> for CommitSumComparator {
    fn extract_key(value: &Commit) -> &Sum {
        &value.statesum
    }
}

/// Holds a set of commits
struct CommitSet {
    commits: HashIndexed<Commit, Sum, CommitSumComparator>
}

impl CommitReceiver for CommitSet {
    fn receive(&mut self, statesum: Sum, parent: Sum,
        changes: HashMap<u64, EltChange>) -> bool {
        
        let key_already_present = !self.commits.insert(
            Commit { statesum: statesum, parent: parent, changes: changes });
        if key_already_present {
            //TODO: what should we do about this case?
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
            println!("Warning: multiple commits reach the same checksum (and \
                presumably state). Some will be ignored.");
        }
        
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

/// Struct holding data used during log replay.
///
/// This stores *all* recreated states since it does not know which may be used
/// as parents of future commits. API currently only allows access to the tip,
/// but could be modified.
struct LogReplay {
    //TODO: use HashIndexed instead of HashMap to avoid storing the key twice.
    // Except we cannot since HashIndexed cannot implement get() !
    states: HashMap<Sum, RepoState>,
    tips: HashSet<Sum>
}

impl LogReplay {
    /// Create the structure from an initial state and sum
    pub fn from_initial(state: RepoState, sum: Sum) -> LogReplay {
        let mut states = HashMap::new();
        states.insert(sum, state);
        let mut tips = HashSet::new();
        tips.insert(sum);
        LogReplay { states: states, tips: tips }
    }
    
    /// Recreate all known states from a set of commits. On success, return a
    /// reference to self. Will fail if a commit applies to an unknown state or
    /// any checksum is incorrect.
    pub fn replay(&mut self, commits: CommitSet) -> Result<&Self> {
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
            self.states.insert(commit.statesum, state);
            
            self.tips.insert(commit.statesum);
            self.tips.remove(&commit.parent);
        }
        Ok(self)
    }
    
    /// Merge all latest states into a single tip.
    pub fn merge(&mut self) -> Result<&Self> {
        //TODO
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
