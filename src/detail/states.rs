//! Pippin: support for dealing with log replay, commit creation, etc.

use std::collections::{HashMap};
use std::collections::hash_map::{Keys};
use std::clone::Clone;
use hashindexed::KeyComparator;
use error::ElementOp;
use rand::random;

use detail::{Sum, ElementT, Element};


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
#[derive(PartialEq, Debug)]
pub struct PartitionState<E: ElementT> {
    part_id: u64,
    parent: Sum,
    statesum: Sum,
    elts: HashMap<EltId, Element<E>>
}

impl<E: ElementT> PartitionState<E> {
    /// Create a new state, with no elements or history.
    /// 
    /// The partition's identifier must be given; this is used to assign new
    /// element identifiers.
    pub fn new(part_id: u64) -> PartitionState<E> {
        PartitionState { part_id: part_id, parent: Sum::zero(),
            statesum: Sum::zero(), elts: HashMap::new() }
    }
    /// Create from a map of elements
    pub fn from_hash_map(part_id: u64, parent: Sum,
        map: HashMap<EltId, Element<E>>, statesum: Sum) -> PartitionState<E>
    {
        PartitionState { part_id: part_id, parent: parent, statesum: statesum, elts: map }
    }
    
    /// Get the state sum
    pub fn statesum(&self) -> &Sum { &self.statesum }
    /// Get the parent's sum
    pub fn parent(&self) -> &Sum { &self.parent }
    
    /// Get access to the map holding elements
    pub fn map(&self) -> &HashMap<EltId, Element<E>> {
        &self.elts
    }
    /// Destroy the PartitionState, extracting its map of elements
    pub fn into_map(self) -> HashMap<EltId, Element<E>> {
        self.elts
    }
    
    /// Get a reference to some element (which can be cloned if required).
    /// 
    /// Note that elements can't be modified directly but must instead be
    /// replaced, hence there is no version of this function returning a
    /// mutable reference.
    pub fn get_elt(&self, id: EltId) -> Option<&Element<E>> {
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
    pub fn elt_ids(&self) -> Keys<EltId, Element<E>> {
        self.elts.keys()
    }
    
    /// Generate an element identifier.
    pub fn gen_id(&self) -> Result<u64, ElementOp> {
        // Generate an identifier: (1) use a random sample, (2) increment if
        // taken, (3) add the partition identifier.
        let initial = (random::<u32>() & 0xFF_FFFF) as u64;
        let mut id = initial;
        while self.elts.contains_key(&id) {
            id += 1;
            if id == 1 << 24 {
                id = 0;
            }
            if id == initial {
                return Err(ElementOp::id_gen_failure());
            }
        }
        Ok(self.part_id + id)
    }
    
    /// Insert an element, generating an identifier.
    /// 
    /// This is a convenience version of `self.insert_elt(try!(self.gen_id(), elt))`.
    /// 
    /// This function should succeed provided that not all usable identifiers
    /// are taken (2^24), though when approaching full it may be slow.
    pub fn new_elt(&mut self, elt: Element<E>) -> Result<(), ElementOp> {
        let id = try!(self.gen_id());
        self.insert_elt(id, elt)
    }
    /// Insert an element and return (). Fails if the id does not have the
    /// correct partition identifier part or if the id is already in use.
    /// It is suggested to use new_elt() instead if you do not need to specify
    /// the identifier.
    pub fn insert_elt(&mut self, id: EltId, elt: Element<E>) -> Result<(), ElementOp> {
//        TODO: elt.cache_classifiers(classifiers);
        if self.elts.contains_key(&id) { return Err(ElementOp::insertion_failure(id)); }
        self.statesum.permute(&elt.sum());
        self.elts.insert(id, elt);
        Ok(())
    }
    /// Replace an existing element and return the replaced element, unless the
    /// id is not already used in which case the function stops with an error.
    /// 
    /// Since elements cannot be edited directly, this is the next best way of
    /// changing an element's contents.
    pub fn replace_elt(&mut self, id: EltId, elt: Element<E>) -> Result<Element<E>, ElementOp> {
//        TODO: elt.cache_classifiers(classifiers);
        self.statesum.permute(&elt.sum());
        match self.elts.insert(id, elt) {
            None => Err(ElementOp::replacement_failure(id)),
            Some(removed) => {
                self.statesum.permute(&removed.sum());
                Ok(removed)
            }
        }
    }
    /// Remove an element, returning the element removed. If no element is
    /// found with the `id` given, `None` is returned.
    pub fn remove_elt(&mut self, id: EltId) -> Result<Element<E>, ElementOp> {
        match self.elts.remove(&id) {
            None => Err(ElementOp::deletion_failure(id)),
            Some(removed) => {
                self.statesum.permute(&removed.sum());
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
            part_id: self.part_id,
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
        PartitionState {
            part_id: self.part_id,
            parent: self.parent.clone(),
            statesum: self.statesum.clone(),
            elts: self.elts.clone() }
    }
}

/// Helper to use PartitionState with HashIndexed
pub struct PartitionStateSumComparator;
impl<E: ElementT> KeyComparator<PartitionState<E>, Sum> for PartitionStateSumComparator {
    fn extract_key(value: &PartitionState<E>) -> &Sum {
        value.statesum()
    }
}
