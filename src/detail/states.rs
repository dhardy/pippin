//! Pippin: support for dealing with log replay, commit creation, etc.

use std::collections::{HashMap};
use std::collections::hash_map::{Keys};
use std::clone::Clone;
use hashindexed::KeyComparator;
use error::ElementOp;

use detail::{Sum, Element};


/// Type of an element identifier within a partition.
pub type EltId = u64;

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
    parent: Sum,
    statesum: Sum,
    elts: HashMap<EltId, Element>
}

impl PartitionState {
    /// Create a new state, with no elements or history.
    /// 
    /// The parent's checksum must be specified.
    pub fn new() -> PartitionState {
        PartitionState { parent: Sum::zero(), statesum: Sum::zero(), elts: HashMap::new() }
    }
    /// Create from a map of elements
    pub fn from_hash_map(parent: Sum,
        map: HashMap<EltId, Element>, statesum: Sum) -> PartitionState
    {
        PartitionState { parent: parent, statesum: statesum, elts: map }
    }
    
    /// Get the state sum
    pub fn statesum(&self) -> &Sum { &self.statesum }
    /// Get the parent's sum
    pub fn parent(&self) -> &Sum { &self.parent }
    
    /// Get access to the map holding elements
    pub fn map(&self) -> &HashMap<EltId, Element> {
        &self.elts
    }
    /// Destroy the PartitionState, extracting its map of elements
    pub fn into_map(self) -> HashMap<EltId, Element> {
        self.elts
    }
    
    /// Get a reference to some element.
    /// 
    /// Note that elements can't be modified directly but must instead be
    /// replaced, hence there is no version of this function returning a
    /// mutable reference.
    pub fn get_elt(&self, id: EltId) -> Option<&Element> {
        self.elts.get(&id)
    }
    /// True if there are no elements
    pub fn is_empty(&self) -> bool { self.elts.is_empty() }
    /// Get the number of elements
    pub fn num_elts(&self) -> usize {
        self.elts.len()
    }
    /// Is an element with a given key present?
    pub fn has_elt(&self, id: EltId) -> bool {
        self.elts.contains_key(&id)
    }
    /// Get the element keys
    pub fn elt_ids(&self) -> Keys<EltId, Element> {
        self.elts.keys()
    }
    
    /// Insert an element and return (), unless the id is already used in
    /// which case the function stops with an error.
    pub fn insert_elt(&mut self, id: EltId, elt: Element) -> Result<(), ElementOp> {
//        TODO: elt.cache_classifiers(classifiers);
        if self.elts.contains_key(&id) { return Err(ElementOp::insertion_failure(id)); }
        self.statesum.permute(elt.sum());
        self.elts.insert(id, elt);
        Ok(())
    }
    /// Replace an existing element and return the replaced element, unless the
    /// id is not already used in which case the function stops with an error.
    pub fn replace_elt(&mut self, id: EltId, elt: Element) -> Result<Element, ElementOp> {
//        TODO: elt.cache_classifiers(classifiers);
        self.statesum.permute(elt.sum());
        match self.elts.insert(id, elt) {
            None => Err(ElementOp::replacement_failure(id)),
            Some(removed) => {
                self.statesum.permute(removed.sum());
                Ok(removed)
            }
        }
    }
    /// Remove an element, returning the element removed. If no element is
    /// found with the `id` given, `None` is returned.
    pub fn remove_elt(&mut self, id: EltId) -> Result<Element, ElementOp> {
        match self.elts.remove(&id) {
            None => Err(ElementOp::deletion_failure(id)),
            Some(removed) => {
                self.statesum.permute(removed.sum());
                Ok(removed)
            }
        }
    }
    
    // Also see #0021 about commit creation.
    
    /// Clone the state, creating a child state. The new state will consider
    /// the current state to be its parent. This is what should be done when
    /// making changes in order to make a new commit.
    /// 
    /// This "clone" will not compare equal to the current one since the
    /// parents are different.
    /// 
    /// Elements are considered Copy-On-Write so cloning the
    /// state is not particularly expensive.
    pub fn clone_child(&self) -> Self {
        PartitionState {
            parent: self.statesum.clone(),
            statesum: self.statesum.clone(),
            elts: self.elts.clone() }
    }
    
    /// Clone the state, creating an exact copy. The new state will have the
    /// same parent as the current one.
    /// 
    /// Elements are considered Copy-On-Write so cloning the
    /// state is not particularly expensive.
    pub fn clone_exact(&self) -> Self {
        PartitionState { parent: self.parent.clone(),
            statesum: self.statesum.clone(),
            elts: self.elts.clone() }
    }
}

/// Helper to use PartitionState with HashIndexed
pub struct PartitionStateSumComparator;
impl KeyComparator<PartitionState, Sum> for PartitionStateSumComparator {
    fn extract_key(value: &PartitionState) -> &Sum {
        value.statesum()
    }
}
