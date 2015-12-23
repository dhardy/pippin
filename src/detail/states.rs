//! Pippin: support for dealing with log replay, commit creation, etc.

use std::collections::{HashMap};
use std::collections::hash_map::{Keys};
use std::clone::Clone;
use hashindexed::KeyComparator;

use detail::{Sum, Element};
use ::{Result, Error};


/// A state of elements within a partition.
/// 
/// Essentially this holds a map of element identifiers to elements plus some
/// machinery to calculate checksums.
///
/// This holds one state. It is fairly cheap to clone one of these; the map of
/// elements must be cloned but elements hold their data in a
/// reference-counted way.
/// 
/// Elements may be inserted, deleted or replaced. Direct modification is not
/// supported.
#[derive(PartialEq,Eq,Debug)]
pub struct PartitionState {
    statesum: Sum,
    elts: HashMap<u64, Element>
}

impl PartitionState {
    /// Create a new state, with no elements
    pub fn new() -> PartitionState {
        PartitionState { statesum: Sum::zero(), elts: HashMap::new() }
    }
    /// Create from a map of elements
    pub fn from_hash_map(map: HashMap<u64, Element>, statesum: Sum) -> PartitionState {
        PartitionState { statesum: statesum, elts: map }
    }
    
    /// Get the state sum
    pub fn statesum(&self) -> Sum { self.statesum }
    /// Get the state sum by reference
    pub fn statesum_ref(&self) -> &Sum { &self.statesum }
    
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
    /// True if there are no elements
    pub fn is_empty(&self) -> bool { self.elts.is_empty() }
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
//        TODO: elt.cache_classifiers(classifiers);
        if self.elts.contains_key(&id) { return Err(Error::arg("insertion conflicts with an existing element")); }
        self.statesum.permute(elt.sum());
        self.elts.insert(id, elt);
        Ok(())
    }
    /// Replace an existing element and return the replaced element, unless the
    /// id is not already used in which case the function stops with an error.
    pub fn replace_elt(&mut self, id: u64, elt: Element) -> Result<Element> {
//        TODO: elt.cache_classifiers(classifiers);
        self.statesum.permute(elt.sum());
        match self.elts.insert(id, elt) {
            None => Err(Error::no_elt("replacement failed: no existing element")),
            Some(removed) => {
                self.statesum.permute(removed.sum());
                Ok(removed)
            }
        }
    }
    /// Remove an element, returning it.
    pub fn remove_elt(&mut self, id: u64) -> Result<Element> {
        match self.elts.remove(&id) {
            None => Err(Error::no_elt("deletion failed: no element")),
            Some(removed) => {
                self.statesum.permute(removed.sum());
                Ok(removed)
            }
        }
    }
}

impl Clone for PartitionState {
    /// Clone the state. Elements are considered Copy-On-Write so cloning the
    /// state is not particularly expensive.
    fn clone(&self) -> Self {
        PartitionState { statesum: self.statesum, elts: self.elts.clone() }
    }
}

/// Helper to use PartitionState with HashIndexed
pub struct PartitionStateSumComparator;
impl KeyComparator<PartitionState, Sum> for PartitionStateSumComparator {
    fn extract_key(value: &PartitionState) -> &Sum {
        value.statesum_ref()
    }
}
