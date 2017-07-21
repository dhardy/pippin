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
//! This module also contains the `StateRead` and `StateWrite` traits which
//! abstract over operations on partition and repository states.

use std::collections::{HashMap};
use std::collections::hash_map as hs;
use std::clone::Clone;
use std::rc::Rc;

use hashindexed::KeyComparator;

use elt::{Element, EltId};
use sum::Sum;
use commit::*;
use error::{ElementOp, PatchOp};

/// Trait abstracting over read operations on the state of a partition or
/// repository.
pub trait StateRead<E: Element> {
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
    /// Note that elements can't be modified directly but must instead be
    /// replaced with a new version, hence there is no version of this function
    /// returning a mutable reference.
    fn get(&self, id: EltId) -> Result<&E, ElementOp> {
        self.get_rc(id).map(|rc| &**rc)
    }
    /// Low-level version of `get(id)`: returns a reference to the
    /// reference-counted wrapped container of the element.
    fn get_rc(&self, id: EltId) -> Result<&Rc<E>, ElementOp>;
}

/// Trait abstracting over write operations on the state of a partition or
/// repository.
pub trait StateWrite<E: Element>: StateRead<E> {
    /// Tries to directly insert an element with a given identifier.
    /// 
    /// The partition part of the identifier must correspond to the current
    /// partition (when called on a single partition) or a loaded partition
    /// (when called on a repository).
    fn insert(&mut self, id: EltId, elt: E) -> Result<EltId, ElementOp> {
        self.insert_rc(id, Rc::new(elt))
    }
    
    /// Low-level version of `insert(elt)`: takes a reference-counted wrapper
    /// of an element.
    fn insert_rc(&mut self, id: EltId, elt: Rc<E>) -> Result<EltId, ElementOp>;
    
    /// Find a free identifier, and insert the element.
    /// 
    /// When called on a repository, this checks the element's classification
    /// to find the correct partition. When called directly on a partition, it
    /// assumes classification is correct.
    fn insert_new(&mut self, elt: E) -> Result<EltId, ElementOp> {
        self.insert_new_rc(Rc::new(elt))
    }
    
    /// Low-level version of `insert_new(elt)`: takes a reference-counted
    /// wrapper of an element.
    fn insert_new_rc(&mut self, elt: Rc<E>) -> Result<EltId, ElementOp>;
    
    /// Remove an existing element, insert a replacement in the same place
    /// and return the removed element.
    /// 
    /// Atomic: makes no changes if there is any error, such as no old element
    /// is found.
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
/// Essentially this holds a map of elements indexed by their identifiers,
/// partition-metadata and commit-metadata.
#[derive(PartialEq, Debug)]
pub struct PartState<E: Element> {
    parents: Vec<Sum>,
    statesum: Sum,
    elts: HashMap<EltId, Rc<E>>,
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
pub struct MutPartState<E: Element> {
    parent: Sum,
    elt_sum: Sum,
    elts: HashMap<EltId, Rc<E>>,
    meta: CommitMetaPartial,
}

// Constructors
impl<E: Element> PartState<E> {
    /// Create a new state, with no elements or history.
    /// 
    /// The partition's identifier must be given; this is used to assign new
    /// element identifiers. Panics if the partition identifier is invalid.
    /// 
    /// Metadata can be customised via `mcm`.
    pub fn new(mcm: &mut MakeCommitMeta) -> PartState<E> {
        let meta = CommitMeta::new_parents(vec![], mcm);
        let metasum = Sum::state_meta_sum(&[], &meta);
        PartState {
            parents: vec![],
            statesum: metasum /* no elts, so statesum = metasum */,
            elts: HashMap::new(),
            meta: meta,
        }
    }
    
    /// Create a `PartState`, specifying most things explicitly.
    /// 
    /// This is for internal use; don't use externally unless you're really
    /// sure of what you're doing.
    pub fn new_explicit(parents: Vec<Sum>,
            elts: HashMap<EltId, Rc<E>>,
            meta: CommitMeta, elt_sum: Sum) -> PartState<E> {
        let metasum = Sum::state_meta_sum(&parents, &meta);
        PartState {
            parents: parents,
            statesum: &metasum ^ &elt_sum,
            elts: elts,
            meta: meta
        }
    }
    
    /// Create a `PartState` from a `MutPartState` and `MakeCommitMeta` trait.
    pub fn from_mut(mut_state: MutPartState<E>, mcm: &mut MakeCommitMeta) -> PartState<E> {
        let meta = CommitMeta::from_partial(mut_state.meta, mcm);
        let parents = vec![mut_state.parent.clone()];
        let metasum = Sum::state_meta_sum(&parents, &meta);
        PartState {
            parents: parents,
            statesum: &mut_state.elt_sum ^ &metasum,
            elts: mut_state.elts,
            meta: meta
        }
    }
    /// Create a `PartState` from a parent `PartState` and a `Commit`.
    pub fn from_state_commit(parent: &PartState<E>, commit: &Commit<E>) ->
            Result<PartState<E>, PatchOp>
    {
        if parent.statesum() != commit.first_parent() { return Err(PatchOp::WrongParent); }
        let mut mut_state = parent.clone_mut();
        commit.apply_mut(&mut mut_state)?;
        
        let metasum = Sum::state_meta_sum(commit.parents(), commit.meta());
        let statesum = &mut_state.elt_sum ^ &metasum;
        if statesum != *commit.statesum() { return Err(PatchOp::PatchApply); }
        
        Ok(PartState {
            parents: commit.parents().to_vec(),
            statesum: statesum,
            elts: mut_state.elts,
            meta: commit.meta().clone()
        })
    }
}

// Methods on PartState, not applicable to RepoState
impl<E: Element> PartState<E> {
    /// Mutate the metadata in order to yield a new `statesum()` while
    /// otherwise not changing the state.
    /// 
    /// Output may be passed to `Commit::mutate_meta()`.
    pub fn mutate_meta(&mut self) -> (u32, Sum) {
        let old_metasum = Sum::state_meta_sum(&self.parents, &self.meta);
        let old_number = self.meta.number();
        self.meta.incr_number();
        if self.meta.number() == old_number {
            panic!("Unable to mutate meta!");   // out of numbers; what can we do?
        }
        let new_metasum = Sum::state_meta_sum(&self.parents, &self.meta);
        self.statesum = &(&self.statesum ^ &old_metasum) ^ &new_metasum;
        (self.meta.number(), self.statesum.clone())
    }
    
    /// Get the state sum (depends on data and metadata)
    pub fn statesum(&self) -> &Sum { &self.statesum }
    /// Get the metadata sum (this is part of the statesum)
    /// 
    /// This is generated on-the-fly.
    pub fn metasum(&self) -> Sum {
        Sum::state_meta_sum(&self.parents, &self.meta)
    }
    /// Get the parents' sums. Normally a state has one parent, but the initial
    /// state has zero and merge outcomes have two (or more).
    pub fn parents(&self) -> &[Sum] { &self.parents }
    /// Get the commit meta-data associated with this state
    pub fn meta(&self) -> &CommitMeta { &self.meta }
    
    /// Iterate over all elements
    pub fn elts_iter(&self) -> EltIter<E> {
        EltIter { iter: self.elts.iter() }
    }
    
    /// As `gen_id()`, but ensure the generated id is free in both self and
    /// another state.
    pub fn gen_id_binary(&self, s2: &PartState<E>) -> Result<EltId, ElementOp> {
        let mut id = EltId::random();;
        for _ in 0..10000 {
            if !self.elts.contains_key(&id) && !s2.elts.contains_key(&id)
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
            parent: self.statesum.clone(),
            elt_sum: self.statesum() ^ &self.metasum(),
            elts: self.elts.clone(),
            meta: CommitMeta::new_partial(self.statesum.clone(), self.meta.clone()),
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
            parents: self.parents.clone(),
            statesum: self.statesum.clone(),
            elts: self.elts.clone(),
            meta: self.meta.clone(),
        }
    }
}
    
impl<E: Element> MutPartState<E> {
    /// Get the parent's sum
    pub fn parent(&self) -> &Sum { &self.parent }
    /// Get the "element sum". This is all element sums combined via XOR. The
    /// partition statesum is this XORed with the metadata sum.
    pub fn elt_sum(&self) -> &Sum { &self.elt_sum }
    
    /// Iterate over all elements
    pub fn elts_iter(&self) -> EltIter<E> {
        EltIter { iter: self.elts.iter() }
    }
    
    /// Get access to (partial) metadata
    pub fn meta(&self) -> &CommitMetaPartial { &self.meta }
    /// Get write access to metadata
    pub fn meta_mut(&mut self) -> &mut CommitMetaPartial { &mut self.meta }
    
    /// Looks for a free element identifier (randomly).
    /// 
    /// Can fail if nearly all ids are used, but this is highly unlikely,
    /// assuming random distribution of ids.
    pub fn free_id(&mut self) -> Result<EltId, ElementOp> {
        self.free_id_near(EltId::random())
    }
    
    /// Looks for a free element identifier near the given starting point.
    /// 
    /// Can fail if nearly all ids are used, but this is highly unlikely,
    /// assuming random distribution of ids.
    pub fn free_id_near(&mut self, mut id: EltId) -> Result<EltId, ElementOp> {
        for _ in 0..10000 {
            if !self.elts.contains_key(&id) {
                return Ok(id);
            }
            id = id.next_elt();
        }
        Err(ElementOp::IdGenFailure)
    }
}

impl<E: Element> StateRead<E> for PartState<E> {
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
        self.elts.get(&id).ok_or(ElementOp::EltNotFound)
    }
}
impl<E: Element> StateRead<E> for MutPartState<E> {
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
        self.elts.get(&id).ok_or(ElementOp::EltNotFound)
    }
}
impl<E: Element> StateWrite<E> for MutPartState<E> {
    fn insert_rc(&mut self, id: EltId, elt: Rc<E>) -> Result<EltId, ElementOp> {
        if self.elts.contains_key(&id) { return Err(ElementOp::IdClash); }
        self.elt_sum.permute(&elt.sum(id));
        self.elts.insert(id, elt);
        Ok(id)
    }
    
    fn insert_new_rc(&mut self, elt: Rc<E>) -> Result<EltId, ElementOp> {
        let id = self.free_id()?;
        self.insert_rc(id, elt)
    }
    
    fn replace_rc(&mut self, id: EltId, elt: Rc<E>) -> Result<Rc<E>, ElementOp> {
        match self.elts.entry(id) {
            hs::Entry::Occupied(ref mut entry) => Ok(entry.insert(elt)),
            hs::Entry::Vacant(_) => Err(ElementOp::EltNotFound),
        }
    }
    
    fn remove(&mut self, id: EltId) -> Result<Rc<E>, ElementOp> {
        match self.elts.remove(&id) {
            None => Err(ElementOp::EltNotFound),
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

/// Helper to use `PartState` with `HashIndexed`
pub struct PartStateSumComparator;
impl<E: Element> KeyComparator<PartState<E>, Sum> for PartStateSumComparator {
    fn extract_key(value: &PartState<E>) -> &Sum {
        value.statesum()
    }
}
