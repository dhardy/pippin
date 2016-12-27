/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Pippin: representation of some *state* of a partition (the latest state or
//! some historical state).
//! 
//! The main components of this module are `PartState` and `MutPartState`,
//! which each hold a set of elements of a partition and provide access to
//! them, along with some metadata (partition identifier, parent commit(s),
//! checksums, commit metadata). `MutPartState` additionally allows
//! modification of the set of elements and updates its checksums as this
//! happens.
//! 
//! This module also contains the `StateT` and `MutStateT` traits which
//! abstract over operations on partition and repository states.

use std::collections::{HashMap};
use std::collections::hash_map as hs;
use std::clone::Clone;
use std::rc::Rc;

use hashindexed::KeyComparator;
use rand::random;

use {ElementT, Sum, PartId, EltId};
use commit::*;
use error::{ElementOp, PatchOp};

/// Trait abstracting over read operations on the state of a partition or
/// repository.
pub trait StateT<E: ElementT> {
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
pub trait MutStateT<E: ElementT>: StateT<E> {
    /// Inserts a new element with a generated identifier then returns that
    /// identifier.
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
    /// Low-level version of `insert(elt)`: takes a reference-counter wrapper
    /// of an element.
    fn insert_rc(&mut self, elt: Rc<E>) -> Result<EltId, ElementOp> {
        // #0049: configurable source of randomness?
        let initial = random::<u32>() & 0xFF_FFFF;
        self.insert_rc_initial(initial, elt)
    }
    /// Low-level version of `insert(elt)` which allows specification of
    /// a number used to generate an identifier. See documentation of
    /// `MutPartState::id_from_initial()` for details.
    fn insert_initial(&mut self, initial: u32, elt: E) -> Result<EltId, ElementOp> {
        self.insert_rc_initial(initial, Rc::new(elt))
    }
    /// Lowest-level version of `insert(elt)`: takes an Rc-wrapped element and
    /// allows specification of a number used to generate an identifier.
    /// See documentation of `MutPartState::id_from_initial()` for details.
    fn insert_rc_initial(&mut self, initial: u32, elt: Rc<E>) -> Result<EltId, ElementOp>;
    
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

/// A 'state' is the set of elements in a partition at some point in time.
/// Partitions have multiple states (the latest and each historical state which
/// has been loaded, possibly also unmerged branches).
/// 
/// This holds one state. It is fairly cheap to clone one of these; the map of
/// elements must be cloned but elements hold their data in a
/// reference-counted way.
/// 
/// Essentially this holds a map of elements indexed by their identifiers, a
/// map of moved elements, partition-metadata and commit-metadata.
#[derive(PartialEq, Debug)]
pub struct PartState<E: ElementT> {
    part_id: PartId,
    parents: Vec<Sum>,
    statesum: Sum,
    elts: HashMap<EltId, Rc<E>>,
    moved: HashMap<EltId, EltId>,
    meta: CommitMeta,
}

/// An editable version of `PartState`.
///
/// Elements may be inserted, deleted or replaced. Direct modification is not
/// supported.
/// 
/// This is a distinct type for two reasons: it is convenient to represent
/// metadata differently, and requiring explicit type conversion ensures that
/// commit creation happens correctly.
/// 
/// Note: there is a possibility that the internal representation be adjusted
/// to a copy of the parent state plus a list of changes, however, it remains
/// to be seen what advantages and disadvantages this would have. See issue
/// #0021.
#[derive(PartialEq, Debug)]
pub struct MutPartState<E: ElementT> {
    part_id: PartId,
    parent: Sum,
    elt_sum: Sum,
    elts: HashMap<EltId, Rc<E>>,
    moved: HashMap<EltId, EltId>,
    meta: CommitMetaPartial,
}

impl<E: ElementT> PartState<E> {
    /// Create a new state, with no elements or history.
    /// 
    /// The partition's identifier must be given; this is used to assign new
    /// element identifiers. Panics if the partition identifier is invalid.
    /// 
    /// Meta-data may be specified via `make_meta` or `None` can be passed.
    pub fn new(part_id: PartId, make_meta: Option<&MakeMeta>) ->
            PartState<E>
    {
        let parents = vec![];
        let meta = CommitMeta::new_parents(&vec![], vec![], make_meta);
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
    
    /// Create a `PartState` from a `MutPartState` and an optional
    /// `MakeMeta` trait.
    pub fn from_mut(mut_state: MutPartState<E>,
            make_meta: Option<&MakeMeta>) -> PartState<E>
    {
        let parents = vec![mut_state.parent.clone()];
        let meta = CommitMeta::from_partial(mut_state.meta, &parents, make_meta);
        let metasum = Sum::state_meta_sum(mut_state.part_id, &parents, &meta);
        PartState {
            part_id: mut_state.part_id,
            parents: parents,
            statesum: &mut_state.elt_sum ^ &metasum,
            elts: mut_state.elts,
            moved: mut_state.moved,
            meta: meta
        }
    }
    /// Create a `PartState` from a parent `PartState` and a `Commit`.
    pub fn from_state_commit(parent: &PartState<E>, commit: &Commit<E>) ->
            Result<PartState<E>, PatchOp>
    {
        if parent.statesum() != commit.first_parent() { return Err(PatchOp::WrongParent); }
        let mut mut_state = parent.clone_mut();
        try!(commit.apply_mut(&mut mut_state));
        
        let metasum = Sum::state_meta_sum(mut_state.part_id, &commit.parents(), &commit.meta());
        let statesum = &mut_state.elt_sum ^ &metasum;
        if statesum != *commit.statesum() { return Err(PatchOp::PatchApply); }
        
        Ok(PartState {
            part_id: mut_state.part_id,
            parents: commit.parents().to_vec(),
            statesum: statesum,
            elts: mut_state.elts,
            moved: mut_state.moved,
            meta: commit.meta().clone()
        })
    }
    
    /// Mutate the metadata in order to yield a new `statesum()` while
    /// otherwise not changing the state.
    /// 
    /// Output may be passed to `Commit::mutate_meta()`.
    pub fn mutate_meta(&mut self) -> (u32, Sum) {
        let old_metasum = Sum::state_meta_sum(self.part_id, &self.parents, &self.meta);
        let old_number = self.meta.number();
        self.meta.incr_number();
        if self.meta.number() == old_number {
            panic!("Unable to mutate meta!");   // out of numbers; what can we do?
        }
        let new_metasum = Sum::state_meta_sum(self.part_id, &self.parents, &self.meta);
        self.statesum = &(&self.statesum ^ &old_metasum) ^ &new_metasum;
        (self.meta.number(), self.statesum.clone())
    }
    
    /// Get the state sum (depends on data and metadata)
    pub fn statesum(&self) -> &Sum { &self.statesum }
    /// Get the metadata sum (this is part of the statesum)
    /// 
    /// This is generated on-the-fly.
    pub fn metasum(&self) -> Sum {
        Sum::state_meta_sum(self.part_id, &self.parents, &self.meta)
    }
    /// Get the parents' sums. Normally a state has one parent, but the initial
    /// state has zero and merge outcomes have two (or more).
    pub fn parents(&self) -> &[Sum] { &self.parents }
    /// Get the partition identifier
    pub fn part_id(&self) -> PartId { self.part_id }
    /// Get the commit meta-data associated with this state
    pub fn meta(&self) -> &CommitMeta { &self.meta }
    
    /// Get the number of elements contained
    pub fn elts_len(&self) -> usize {
        self.elts.len()
    }
    /// Iterate over all elements
    pub fn elts_iter(&self) -> EltIter<E> {
        EltIter { iter: self.elts.iter() }
    }
    /// Get a specific element, if contained
    pub fn elt(&self, id: EltId) -> Option<&Rc<E>> {
        self.elts.get(&id)
    }
    
    /// Get the number of "moved" elements.
    /// 
    /// This is a record of the identifiers of each element which was once in
    /// this partition, but has been moved. It maps to a new identifier. You
    /// can use `new_elt_id.part_id()` to find the new partition.
    pub fn moved_len(&self) -> usize {
        self.moved.len()
    }
    /// Iterate over all moved element records
    /// 
    /// This is a record of the identifiers of each element which was once in
    /// this partition, but has been moved. It maps to a new identifier. You
    /// can use `new_elt_id.part_id()` to find the new partition.
    pub fn moved_iter(&self) -> EltIdIter {
        EltIdIter { iter: self.moved.iter() }
    }
    
    /// Check our notes tracking moved elements, and return a new `EltId` if
    /// we have one. Note that this method ignores stored elements.
    pub fn is_moved(&self, id: EltId) -> Option<EltId> {
        self.moved.get(&id).map(|id| *id) // Some(value) or None
    }
    
    /// As `gen_id()`, but ensure the generated id is free in both self and
    /// another state. Note that the other state is assumed to have the same
    /// `part_id`.
    pub fn gen_id_binary(&self, s2: &PartState<E>) -> Result<EltId, ElementOp> {
        assert_eq!(self.part_id, s2.part_id);
        // #0049: configurable source of randomness?
        let initial = self.part_id.elt_id(random::<u32>() & 0xFF_FFFF);
        let mut id = initial;
        for _ in 0..10000 {
            if !self.elts.contains_key(&id) && !s2.elts.contains_key(&id) &&
                !self.moved.contains_key(&id) && !s2.moved.contains_key(&id)
            {
                return Ok(id)
            }
            id = id.next_elt();
        }
        Err(ElementOp::IdGenFailure)
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
            meta: CommitMeta::new_partial(&self.meta),
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
    
    /// Get the number of elements contained
    pub fn elts_len(&self) -> usize {
        self.elts.len()
    }
    /// Iterate over all elements
    pub fn elts_iter(&self) -> EltIter<E> {
        EltIter { iter: self.elts.iter() }
    }
    /// Get a specific element, if contained
    pub fn elt(&self, id: EltId) -> Option<&Rc<E>> {
        self.elts.get(&id)
    }
    
    /// Get the number of "moved" elements.
    /// 
    /// This is a record of the identifiers of each element which was once in
    /// this partition, but has been moved. It maps to a new identifier. You
    /// can use `new_elt_id.part_id()` to find the new partition.
    pub fn moved_len(&self) -> usize {
        self.moved.len()
    }
    /// Iterate over all moved element records
    /// 
    /// This is a record of the identifiers of each element which was once in
    /// this partition, but has been moved. It maps to a new identifier. You
    /// can use `new_elt_id.part_id()` to find the new partition.
    pub fn moved_iter(&self) -> EltIdIter {
        EltIdIter { iter: self.moved.iter() }
    }
    
    /// Check our notes tracking moved elements, and return a new `EltId` if
    /// we have one. Note that this method ignores stored elements.
    pub fn is_moved(&self, id: EltId) -> Option<EltId> {
        self.moved.get(&id).map(|id| *id) // Some(value) or None
    }
    
    /// Find a free element identifier, given an initial number. This number
    /// must be between zero and `EltId::max()`. An initial element identifier
    /// will be generated from the partition number (`part_id.elt_id(initial)`)
    /// which will be returned if free, otherwise the function searches nearby
    /// deterministically for a free identifier.
    /// 
    /// Note that the search algorithm is desiged to handle the case that the
    /// initial number is fairly well distributed (e.g. uniform random) and
    /// there are quite a few free identifiers. It is not designed to find the
    /// last few free identifiers efficiently, and it can fail.
    pub fn id_from_initial(&self, initial: u32) -> Result<EltId, ElementOp> {
        let mut id = self.part_id.elt_id(initial);
        for _ in 0..10000 {
            if !self.elts.contains_key(&id) && !self.moved.contains_key(&id) {
                return Ok(id);
            }
            id = id.next_elt();
        }
        Err(ElementOp::IdGenFailure)
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
    
    /// Get access to (partial) metadata
    pub fn meta(&self) -> &CommitMetaPartial { &self.meta }
    /// Get write access to metadata
    pub fn meta_mut(&mut self) -> &mut CommitMetaPartial { &mut self.meta }
}

impl<E: ElementT> StateT<E> for PartState<E> {
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
impl<E: ElementT> StateT<E> for MutPartState<E> {
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
impl<E: ElementT> MutStateT<E> for MutPartState<E> {
    fn insert_rc_initial(&mut self, initial: u32, elt: Rc<E>) -> Result<EltId, ElementOp> {
        let id = try!(self.id_from_initial(initial));
        self.insert_with_id(id, elt)
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

/// Wrapper around underlying iterator structure
pub struct EltIter<'a, E: 'a> {
    iter: hs::Iter<'a, EltId, Rc<E>>
}
impl<'a, E> Clone for EltIter<'a, E> {
    fn clone(&self) -> EltIter<'a, E> {
        EltIter { iter: self.iter.clone() }
    }
}
impl<'a, E> Iterator for EltIter<'a, E> {
    type Item = (EltId, &'a Rc<E>);
    fn next(&mut self) -> Option<(EltId, &'a Rc<E>)> {
        self.iter.next().map(|(k,v)| (*k, v))
    }
}
impl<'a, E> ExactSizeIterator for EltIter<'a, E> {
    fn len(&self) -> usize {
        self.iter.len()
    }
}

/// Wrapper around underlying iterator structure
pub struct EltIdIter<'a> {
    iter: hs::Iter<'a, EltId, EltId>
}
impl<'a> Clone for EltIdIter<'a> {
    fn clone(&self) -> EltIdIter<'a> {
        EltIdIter { iter: self.iter.clone() }
    }
}
impl<'a> Iterator for EltIdIter<'a> {
    type Item = (EltId, EltId);
    fn next(&mut self) -> Option<(EltId, EltId)> {
        self.iter.next().map(|(k,v)| (*k, *v))
    }
}
impl<'a> ExactSizeIterator for EltIdIter<'a> {
    fn len(&self) -> usize {
        self.iter.len()
    }
}

/// Helper to use PartState with HashIndexed
pub struct PartStateSumComparator;
impl<E: ElementT> KeyComparator<PartState<E>, Sum> for PartStateSumComparator {
    fn extract_key(value: &PartState<E>) -> &Sum {
        value.statesum()
    }
}
