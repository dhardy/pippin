//! Pippin: support for dealing with log replay, commit creation, etc.

use std::collections::{HashMap};
use std::collections::hash_map::{Keys};
use std::clone::Clone;
use std::rc::Rc;

use hashindexed::KeyComparator;
use rand::random;

use {ElementT, Sum, PartId, EltId};
use error::ElementOp;


/// Type of map used internally
pub type EltMap<E> = HashMap<EltId, Rc<E>>;

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
    part_id: PartId,
    parent: Sum,
    statesum: Sum,
    elts: EltMap<E>,
    moved: HashMap<EltId, EltId>,
}

impl<E: ElementT> PartitionState<E> {
    /// Create a new state, with no elements or history.
    /// 
    /// The partition's identifier must be given; this is used to assign new
    /// element identifiers. Panics if the partition identifier is invalid.
    pub fn new(part_id: PartId) -> PartitionState<E> {
        PartitionState {
            part_id: part_id,
            parent: Sum::zero(),
            statesum: Sum::zero(),
            elts: HashMap::new(),
            moved: HashMap::new(),
        }
    }
    
    /// Get the state sum
    pub fn statesum(&self) -> &Sum { &self.statesum }
    /// Get the parent's sum
    pub fn parent(&self) -> &Sum { &self.parent }
    
    /// Get access to the map holding elements
    pub fn map(&self) -> &EltMap<E> {
        &self.elts
    }
    /// Destroy the PartitionState, extracting its maps
    /// 
    /// First is map of elements (`self.map()`), second is map of moved elements
    /// (`self.moved_map()`).
    pub fn into_maps(self) -> (EltMap<E>, HashMap<EltId, EltId>) {
        (self.elts, self.moved)
    }
    /// Get access to the map of moved elements to new identifiers
    pub fn moved_map(&self) -> &HashMap<EltId, EltId> {
        &self.moved
    }
    
    /// Get some element, still in its Element wrapper (which can be cloned if required).
    /// 
    /// Note that elements can't be modified directly but must instead be
    /// replaced, hence there is no version of this function returning a
    /// mutable reference.
    pub fn get_rc(&self, id: EltId) -> Option<&Rc<E>> {
        self.elts.get(&id)
    }
    /// Get a reference to some element (which can be cloned if required).
    /// 
    /// Note that elements can't be modified directly but must instead be
    /// replaced, hence there is no version of this function returning a
    /// mutable reference.
    pub fn get_elt(&self, id: EltId) -> Option<&E> {
        self.elts.get(&id).map(|rc| &**rc)
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
    pub fn elt_ids(&self) -> Keys<EltId, Rc<E>> {
        self.elts.keys()
    }
    
    /// Generate an element identifier.
    /// 
    /// This generates a pseudo-random number
    pub fn gen_id(&self) -> Result<EltId, ElementOp> {
        // Generate an identifier: (1) use a random sample, (2) increment if
        // taken, (3) add the partition identifier.
        let initial = self.part_id.elt_id(random::<u32>() & 0xFF_FFFF);
        let mut id = initial;
        loop {
            if !self.elts.contains_key(&id) && !self.moved.contains_key(&id) { break; }
            id = id.next_elt();
            //TODO: is this too many to check exhaustively? We could use a
            // lower limit, and possibly resample a few times.
            if id == initial {
                return Err(ElementOp::id_gen_failure());
            }
        }
        Ok(id)
    }
    /// As `gen_id()`, but ensure the generated id is free in both self and
    /// another state. Note that the other state is assumed to have the same
    /// `part_id`; if not this is equivalent to `gen_id()`.
    pub fn gen_id_binary(&self, s2: &PartitionState<E>) -> Result<EltId, ElementOp> {
        let mut id = try!(self.gen_id());
        let mut tries = 1000;
        loop {
            if !self.elts.contains_key(&id) && !s2.elts.contains_key(&id) &&
                !self.moved.contains_key(&id) && !s2.moved.contains_key(&id)
            {
                break;
            }
            id = id.next_elt();
            tries -= 1;
            if tries == 0 {
                return Err(ElementOp::id_gen_failure());
            }
        }
        Ok(id)
    }
    
    /// Insert an element, generating an identifier. Returns the new
    /// identifier on success.
    /// 
    /// This is a convenience version of `self.insert_elt(try!(self.gen_id(), elt))`.
    /// 
    /// This function should succeed provided that not all usable identifiers
    /// are taken (2^24), though when approaching full it may be slow.
    pub fn new_elt(&mut self, elt: E) -> Result<EltId, ElementOp> {
        self.new_rc(Rc::new(elt))
    }
    /// As `new_elt()`, but accept an Rc-wrapped element.
    pub fn new_rc(&mut self, elt: Rc<E>) -> Result<EltId, ElementOp> {
        let id = try!(self.gen_id());
        try!(self.insert_rc(id, elt));
        Ok(id)
    }
    /// Insert an element and return the id (the one inserted).
    /// 
    /// Fails if the id does not have the correct partition identifier part or
    /// if the id is already in use.
    /// It is suggested to use new_elt() instead if you do not need to specify
    /// the identifier.
    pub fn insert_elt(&mut self, id: EltId, elt: E) -> Result<EltId, ElementOp> {
        try!(self.insert_rc(id, Rc::new(elt)));
        Ok(id)
    }
    /// As `insert_elt()`, but accept an Rc-wrapped element.
    pub fn insert_rc(&mut self, id: EltId, elt: Rc<E>) -> Result<EltId, ElementOp> {
        if self.elts.contains_key(&id) { return Err(ElementOp::insertion_failure(id)); }
        self.statesum.permute(&elt.sum());
        self.elts.insert(id, elt);
        Ok(id)
    }
    /// Replace an existing element and return the replaced element, unless the
    /// id is not already used in which case the function stops with an error.
    /// 
    /// Since elements cannot be edited directly, this is the next best way of
    /// changing an element's contents.
    /// 
    /// Note that the returned `Rc<E>` cannot be unwrapped automatically since
    /// we do not know that we have the only reference.
    pub fn replace_elt(&mut self, id: EltId, elt: E) -> Result<Rc<E>, ElementOp> {
        self.replace_rc(id, Rc::new(elt))
    }
    /// As `replace_elt()`, but accept an Rc-wrapped element.
    pub fn replace_rc(&mut self, id: EltId, elt: Rc<E>) -> Result<Rc<E>, ElementOp> {
        self.statesum.permute(&elt.sum());
        match self.elts.insert(id, elt) {
            None => Err(ElementOp::replacement_failure(id)),
            Some(removed) => {
                self.statesum.permute(&removed.sum());
                Ok(removed)
            }
        }
    }
    /// Remove an element, returning the element removed or failing.
    pub fn remove_elt(&mut self, id: EltId) -> Result<Rc<E>, ElementOp> {
        match self.elts.remove(&id) {
            None => Err(ElementOp::deletion_failure(id)),
            Some(removed) => {
                self.statesum.permute(&removed.sum());
                Ok(removed)
            }
        }
    }
    /// Remove and return an element, but leave a memo that it has been moved
    /// to another identifier (usually under another partition).
    pub fn remove_to(&mut self, id: EltId, new_id: EltId) -> Result<Rc<E>, ElementOp> {
        let removed = try!(self.remove_elt(id));
        self.moved.insert(id, new_id);
        Ok(removed)
    }
    
    /// Check our notes tracking moved elements, and return a new `EltId` if
    /// we have one. Note that this method ignores stored elements.
    pub fn is_moved(&self, id: EltId) -> Option<EltId> {
        self.moved.get(&id).map(|id| *id) // Some(value) or None
    }
    /// Update notes about where an element has been moved to (like
    /// `remove_to()` but without trying to remove the element). Used both to
    /// notify that an already-moved element has been moved again, and when
    /// reading stored data.
    /// 
    /// This *should* be called when an element is moved again, but probably
    /// won't be until `locate()` is called since we don't currently track
    /// elements' old identities.
    /// 
    /// In the case the element has been moved back to this partition, the
    /// current code may or may not give it its original identity back
    /// (depending on whether the element number part has already been
    /// changed).
    /// 
    /// In theory a prerequisite of calling this should be that there is
    /// already a note that `id` was moved, but we have no reason to enforce
    /// this so do not. `self.is_moved(id)` is the only method which checks
    /// these notes in any case.
    pub fn set_move(&mut self, id: EltId, new_id: EltId) {
        self.moved.insert(id, new_id);
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
            elts: self.elts.clone(),
            moved: self.moved.clone(),
        }
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
            elts: self.elts.clone(),
            moved: self.moved.clone(),
        }
    }
}

/// Helper to use PartitionState with HashIndexed
pub struct PartitionStateSumComparator;
impl<E: ElementT> KeyComparator<PartitionState<E>, Sum> for PartitionStateSumComparator {
    fn extract_key(value: &PartitionState<E>) -> &Sum {
        value.statesum()
    }
}
