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
use std::hash::{Hash, Hasher};

use detail::{Sum, Element};
use detail::readwrite::CommitReceiver;

/// State of the repository (see module doc)
#[derive(PartialEq,Eq,Debug)]
pub struct RepoState {
    pub elts: HashMap<u64, Element>
}

impl RepoState {
    /// Create a new state, with no elements
    pub fn new() -> RepoState {
        RepoState { elts: HashMap::new() }
    }
    /// Create from a map of elements
    pub fn from_hash_map(map: HashMap<u64, Element>) -> RepoState {
        RepoState { elts: map }
    }
    
    /// Get access to the map holding elements
    pub fn map(&self) -> &HashMap<u64, Element> {
        &self.elts
    }
    /// Get a reference to some element
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
    /// Insert an element and return true, unless the id is already used in
    /// which case the function does nothing but return false.
    pub fn insert_elt(&mut self, id: u64, elt: Element) -> bool {
        if self.elts.contains_key(&id) { return false; }
        self.elts.insert(id, elt);
        true
    }
}


/// Holds a set of commits
struct CommitSet {
    commits: HashSet<Commit>
}

impl CommitReceiver for CommitSet {
    fn receive(&mut self, statesum: Sum, parent: Sum,
        changes: HashMap<u64, EltChange>) -> bool {
        
        let key_already_present = !self.commits.insert(
            Commit { statesum: statesum, parent: parent,
            timestamp: (), changes: changes });
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
    /// Time when this commit was made (TODO)
    timestamp: (),
    /// Per-element changes
    changes: HashMap<u64, EltChange>
}

impl PartialEq<Commit> for Commit {
    fn eq(&self, other: &Commit) -> bool {
        //TODO: should we *assert* that other fields of self and other and
        // equal? If two paths to the same result exist, we may silently drop
        // one of those paths. Provided the result *is* the same (not a hash
        // collision), presumably this is okay...
        self.statesum == other.statesum
    }
}
impl Eq for Commit {}
impl Hash for Commit {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.statesum.hash(state)
    }
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
