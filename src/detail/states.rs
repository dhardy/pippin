/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Pippin: support for dealing with log replay, commit creation, etc.

use std::collections::{HashMap};
use std::clone::Clone;
use std::rc::Rc;

use hashindexed::KeyComparator;
use rand::random;

use {ElementT, Sum, PartId, EltId, CommitMeta};
use error::ElementOp;

/// Trait abstracting over read operations on the state of a partition or
/// repository.
pub trait State<E: ElementT> {
    /// Returns true when any elements are available.
    /// 
    /// In a single partition this means that the partition is not empty; in a
    /// repository it means that at least one *loaded* partition is not empty.
    fn any_avail(&self) -> bool;
    /// Returns the number of elements available.
    /// 
    /// In a single partition this is the number of elements contained; in a
    /// repository it is the number of elements contained in *loaded*
    /// partitions.
    fn num_avail(&self) -> usize;
    /// Returns true if and only if an element with a given key is available.
    /// 
    /// Note that this only refers to *in-memory* partitions. If the element in
    /// question is contained in a partition which is not loaded or not
    /// contained in the "repo state" in question, this will return false.
    fn is_avail(&self, id: EltId) -> bool;
    
    /// Get a reference to some element (which can be cloned if required).
    /// 
    /// This fails if the relevant partition is not loaded or the element is
    /// not found. In the case of a multi-partition repository it is possible
    /// that the element has been moved, in which case `RepoState::locate(id)`
    /// may be helpful.
    /// 
    /// Note that elements can't be modified directly but must instead be
    /// replaced with a new version, hence there is no version of this function
    /// returning a mutable reference.
    fn get(&self, id: EltId) -> Result<&E, ElementOp> {
        self.get_rc(id).map(|rc| &**rc)
    }
    /// Low-level version of `get(id)`: returns a reference to the
    /// reference-counter wrapped container of the element.
    fn get_rc(&self, id: EltId) -> Result<&Rc<E>, ElementOp>;
}

/// Trait abstracting over write operations on the state of a partition or
/// repository.
pub trait MutState<E: ElementT>: State<E> {
    /// Insert a new element and return the identifier.
    /// 
    /// This fails if the relevant partition is not loaded or if the relevant
    /// partition is unable to find a free identifier. In the latter case
    /// (`ElementOp::IdGenFailure`) presumably the partition is rather full,
    /// however simply trying again may succeed.
    /// 
    /// Note: on a single partition, the lower-level function `insert(id, elt)`
    /// allows the identifier to be speciifed. On a repository this is not
    /// allowed since the partition is determined automatically and the
    /// partition number becomes part of the element identifier.
    fn insert(&mut self, elt: E) -> Result<EltId, ElementOp> {
        self.insert_rc(Rc::new(elt))
    }
    /// Low-level version of `insert(id)`: takes a reference-counter wrapper
    /// for an element.
    fn insert_rc(&mut self, elt: Rc<E>) -> Result<EltId, ElementOp>;
    
    /// Replace an existing element and return the identifier of the newly
    /// inserted element and the replaced element. Note that the identifier
    /// returned can be different on a repository where the replacement element
    /// is classified under a different partition.
    /// 
    /// This fails if the relevant partition is not loaded or the element is
    /// not found. In the case of a multi-partition repository it is possible
    /// that the element has been moved, in which case `RepoState::locate(id)`
    /// may be helpful.
    /// 
    /// Note that the returned `Rc<E>` cannot be unwrapped automatically since
    /// we do not know that we have the only reference.
    fn replace(&mut self, id: EltId, elt: E) -> Result<Rc<E>, ElementOp> {
        self.replace_rc(id, Rc::new(elt))
    }
    /// Low-level version of `replace(id, elt)` which takes an Rc-wrapped
    /// element.
    fn replace_rc(&mut self, id: EltId, elt: Rc<E>) -> Result<Rc<E>, ElementOp>;
    
    /// Remove an element, returning the element removed or failing.
    /// 
    /// This fails if the relevant partition is not loaded or the element is
    /// not found. In the case of a multi-partition repository it is possible
    /// that the element has been moved, in which case `RepoState::locate(id)`
    /// may be helpful.
    /// 
    /// Note that the returned `Rc<E>` cannot be unwrapped automatically since
    /// we do not know that we have the only reference.
    fn remove(&mut self, id: EltId) -> Result<Rc<E>, ElementOp>;
}

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
pub struct PartState<E: ElementT> {
    part_id: PartId,
    parents: Vec<Sum>,
    statesum: Sum,
    elts: HashMap<EltId, Rc<E>>,
    moved: HashMap<EltId, EltId>,
    meta: CommitMeta,
}

/// An editable state
#[derive(PartialEq, Debug)]
pub struct MutPartState<E: ElementT> {
    part_id: PartId,
    parent: Sum,
    elt_sum: Sum,
    elts: HashMap<EltId, Rc<E>>,
    moved: HashMap<EltId, EltId>,
    number: u32,
}

impl<E: ElementT> PartState<E> {
    /// Create a new state, with no elements or history.
    /// 
    /// The partition's identifier must be given; this is used to assign new
    /// element identifiers. Panics if the partition identifier is invalid.
    pub fn new(part_id: PartId) -> PartState<E> {
        Self::new_with(part_id, Vec::new(), CommitMeta::now_empty())
    }
    /// As `new()`, but letting the user specify commit meta-data and parents.
    pub fn new_with(part_id: PartId, parents: Vec<Sum>, meta: CommitMeta) ->
            PartState<E>
    {
        let metasum = Sum::state_meta_sum(part_id, &parents, &meta);
        PartState {
            part_id: part_id,
            parents: parents,
            statesum: metasum /* no elts, so statesum = metasum */,
            elts: HashMap::new(),
            moved: HashMap::new(),
            meta: meta,
        }
    }
    
    /// Create a `PartState`, specifying most things explicitly.
    /// 
    /// This is for internal use; don't use externally unless you're really
    /// sure of what you're doing.
    pub fn new_explicit(part_id: PartId, parents: Vec<Sum>,
            elts: HashMap<EltId, Rc<E>>, moves: HashMap<EltId, EltId>,
            meta: CommitMeta, elt_sum: Sum) -> PartState<E> {
        let metasum = Sum::state_meta_sum(part_id, &parents, &meta);
        PartState {
            part_id: part_id,
            parents: parents,
            statesum: &metasum ^ &elt_sum,
            elts: elts,
            moved: moves,
            meta: meta
        }
    }
    
    /// Create a `PartState` from a `MutPartState` and metadata.
    /// 
    /// The metadata can be constructed using the number in `mut_state` and
    /// `CommitMeta::timestamp_now()` if not already available.
    /// 
    /// Commit metadata is constructed from the number in the passed
    /// `MutPartState`, the time now and the passed optional extra data.
    pub fn from_mut(mut_state: MutPartState<E>, parents: Vec<Sum>,
            metadata: CommitMeta) -> PartState<E>
    {
        let metasum = Sum::state_meta_sum(mut_state.part_id, &parents, &metadata);
        PartState {
            part_id: mut_state.part_id,
            parents: parents,
            statesum: &mut_state.elt_sum ^ &metasum,
            elts: mut_state.elts,
            moved: mut_state.moved,
            meta: metadata
        }
    }
    
    /// Get the state sum
    pub fn statesum(&self) -> &Sum { &self.statesum }
    /// Get the metadata sum (this is part of the statesum)
    /// 
    /// This is generated on-the-fly.
    pub fn metasum(&self) -> Sum {
        Sum::state_meta_sum(self.part_id, &self.parents, &self.meta)
    }
    /// Get the parents' sums. Normally a state has one parent, but the initial
    /// state has zero and merge outcomes have two (or more).
    /// 
    /// Note: 'parents' is not persisted by snapshots; currently it doesn't
    /// need to be.
    pub fn parents(&self) -> &Vec<Sum> { &self.parents }
    /// Get the partition identifier
    pub fn part_id(&self) -> PartId { self.part_id }
    /// Get the commit meta-data associated with this state
    pub fn meta(&self) -> &CommitMeta { &self.meta }
    
    /// Get access to the map holding elements
    pub fn elt_map(&self) -> &HashMap<EltId, Rc<E>> {
        &self.elts
    }
    /// Get access to the map of moved elements to new identifiers
    pub fn moved_map(&self) -> &HashMap<EltId, EltId> {
        &self.moved
    }
    // Also see #0021 about commit creation.
    
    /// Check our notes tracking moved elements, and return a new `EltId` if
    /// we have one. Note that this method ignores stored elements.
    pub fn is_moved(&self, id: EltId) -> Option<EltId> {
        self.moved.get(&id).map(|id| *id) // Some(value) or None
    }
    
    /// As `gen_id()`, but ensure the generated id is free in both self and
    /// another state. Note that the other state is assumed to have the same
    /// `part_id`; if not this is equivalent to `gen_id()`.
    pub fn gen_id_binary(&self, s2: &PartState<E>) -> Result<EltId, ElementOp> {
        assert_eq!(self.part_id, s2.part_id);
        let initial = self.part_id.elt_id(random::<u32>() & 0xFF_FFFF);
        let mut id = initial;
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
                return Err(ElementOp::IdGenFailure);
            }
        }
        Ok(id)
    }
    
    /// Clone the state, creating a child state. The new state will consider
    /// the current state to be its parent. This is what should be done when
    /// making changes in order to make a new commit.
    /// 
    /// This "clone" will not compare equal to the current one since the
    /// parents are different.
    /// 
    /// Elements are considered Copy-On-Write so cloning the
    /// state is not particularly expensive.
    pub fn clone_mut(&self) -> MutPartState<E> {
        MutPartState {
            part_id: self.part_id,
            parent: self.statesum.clone(),
            elt_sum: self.statesum() ^ &self.metasum(),
            elts: self.elts.clone(),
            moved: self.moved.clone(),
            number: self.meta.next_number(),
        }
    }
    
    /// Clone the state, creating an exact copy. The new state will have the
    /// same parents as the current one.
    /// 
    /// Elements are considered Copy-On-Write so cloning the
    /// state is not particularly expensive (though the hash-map of elements
    /// and a few other bits must still be copied).
    pub fn clone_exact(&self) -> Self {
        PartState {
            part_id: self.part_id,
            parents: self.parents.clone(),
            statesum: self.statesum.clone(),
            elts: self.elts.clone(),
            moved: self.moved.clone(),
            meta: self.meta.clone(),
        }
    }
}
    
impl<E: ElementT> MutPartState<E> {
    /// Get the partition identifier
    pub fn part_id(&self) -> PartId { self.part_id }
    /// Get the parent's sum
    pub fn parent(&self) -> &Sum { &self.parent }
    /// Get the "element sum". This is all element sums combined via XOR. The
    /// partition statesum is this XORed with the metadata sum.
    pub fn elt_sum(&self) -> &Sum { &self.elt_sum }
    
    /// Get access to the map holding elements
    pub fn elt_map(&self) -> &HashMap<EltId, Rc<E>> {
        &self.elts
    }
    /// Get access to the map of moved elements to new identifiers
    pub fn moved_map(&self) -> &HashMap<EltId, EltId> {
        &self.moved
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
            // #0019: is this too many to check exhaustively? We could use a
            // lower limit, and possibly resample a few times.
            // Note that gen_id_binary uses a slightly different algorithm.
            if id == initial {
                return Err(ElementOp::IdGenFailure);
            }
        }
        Ok(id)
    }
    
    /// Insert an element and return the id (the one inserted).
    /// 
    /// Fails if the id does not have the correct partition identifier part or
    /// if the id is already in use.
    /// It is suggested to use insert() instead if you do not need to specify
    /// the identifier.
    pub fn insert_with_id(&mut self, id: EltId, elt: Rc<E>) -> Result<EltId, ElementOp> {
        if id.part_id() != self.part_id { return Err(ElementOp::WrongPartition); }
        if self.elts.contains_key(&id) { return Err(ElementOp::IdClash); }
        self.elt_sum.permute(&elt.sum(id));
        self.elts.insert(id, elt);
        Ok(id)
    }
    
    /// Add a note about where an element has been moved to.
    /// 
    /// The point of doing this is that someone looking for the element later
    /// can find out via `is_moved(old_id)` where an element has been moved to.
    /// 
    /// This should be used when an element is moved to another partition,
    /// after calling `remove_elt()` on this partition. It can also be used
    /// when an element which was here has been moved *again* to inform of the
    /// current name (though this is not currently easy to do, since we don't
    /// track elements' old names).
    /// 
    /// In the case the element has been moved back to this partition, the
    /// current code may or may not give it its original identity back
    /// (depending on whether the element number part has already been
    /// changed).
    pub fn set_move(&mut self, id: EltId, new_id: EltId) {
        self.moved.insert(id, new_id);
    }
    /// Check our notes tracking moved elements, and return a new `EltId` if
    /// we have one. Note that this method ignores stored elements.
    pub fn is_moved(&self, id: EltId) -> Option<EltId> {
        self.moved.get(&id).map(|id| *id) // Some(value) or None
    }
}

impl<E: ElementT> State<E> for PartState<E> {
    fn any_avail(&self) -> bool {
        !self.elts.is_empty()
    }
    fn num_avail(&self) -> usize {
        self.elts.len()
    }
    fn is_avail(&self, id: EltId) -> bool {
        self.elts.contains_key(&id)
    }
    fn get_rc(&self, id: EltId) -> Result<&Rc<E>, ElementOp> {
        self.elts.get(&id).ok_or(ElementOp::NotFound)
    }
}
impl<E: ElementT> State<E> for MutPartState<E> {
    fn any_avail(&self) -> bool {
        !self.elts.is_empty()
    }
    fn num_avail(&self) -> usize {
        self.elts.len()
    }
    fn is_avail(&self, id: EltId) -> bool {
        self.elts.contains_key(&id)
    }
    fn get_rc(&self, id: EltId) -> Result<&Rc<E>, ElementOp> {
        self.elts.get(&id).ok_or(ElementOp::NotFound)
    }
}
impl<E: ElementT> MutState<E> for MutPartState<E> {
    fn insert_rc(&mut self, elt: Rc<E>) -> Result<EltId, ElementOp> {
        let id = try!(self.gen_id());
        try!(self.insert_with_id(id, elt));
        Ok(id)
    }
    fn replace_rc(&mut self, id: EltId, elt: Rc<E>) -> Result<Rc<E>, ElementOp> {
        self.elt_sum.permute(&elt.sum(id));
        match self.elts.insert(id, elt) {
            None => Err(ElementOp::NotFound),
            Some(removed) => {
                self.elt_sum.permute(&removed.sum(id));
                Ok(removed)
            }
        }
    }
    fn remove(&mut self, id: EltId) -> Result<Rc<E>, ElementOp> {
        match self.elts.remove(&id) {
            None => Err(ElementOp::NotFound),
            Some(removed) => {
                self.elt_sum.permute(&removed.sum(id));
                Ok(removed)
            }
        }
    }
}

/// Helper to use PartState with HashIndexed
pub struct PartStateSumComparator;
impl<E: ElementT> KeyComparator<PartState<E>, Sum> for PartStateSumComparator {
    fn extract_key(value: &PartState<E>) -> &Sum {
        value.statesum()
    }
}
