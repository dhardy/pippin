/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Support for merging two conflicting changes
//! 
//! Merge algorithms
//! ==========
//! 
//! Definition: we consider *merging* the process of combining multiple 'tip'
//! states (states with no child state) into one 'tip' or latest state.
//! 
//! We do this by making the assumption that there is no required correlation
//! between elements, thus when updating one element we do not have to
//! consider other elements.
//! 
//! There are several methods by which merges could happen:
//! 
//! *   Take two states and a common ancestor and merge
//! *   Take two states and have a user choose how to merge (no ancestor)
//! *   Take `n` states at once and merge, perhaps with help from the user,
//!     with or without some partially-common ancestor states
//! 
//! And there are several strategies which could be used:
//! 
//! *   Select any two states, merge, recurse until only one state is left
//! *   As above, but let the user choose which states to merge
//! *   Everything at once with an 'n-to-one' merge method
//! 
//! For simplicity we currently only implement two-to-one merge with a common
//! ancestor, recursively selecting two states to merge. Various solvers are
//! available, but for conflicting changes to a single element either a naive
//! solver must be used or a custom solver supplied.

use std::collections::HashMap;
use std::marker::PhantomData;
use std::rc::Rc;

use commit::{Commit, CommitMeta, EltChange, MakeCommitMeta};
use state::{PartState, StateRead};
use elt::{EltId, Element};
use sum::Sum;

/// This struct controls the merging of two states into one.
/// 
/// It currently requires a common ancestor, but could be rewritten not to
/// (by asking for help solving far more cases).
pub struct TwoWayMerge<'a, E: Element+'a> {
    // First tip
    a: &'a PartState<E>,
    // Second tip
    b: &'a PartState<E>,
    // Common ancestor
    c: &'a PartState<E>,
    // List of conflicts
    v: Vec<(EltId, EltMerge<E>)>,
}
impl<'a, E: Element> TwoWayMerge<'a, E> {
    /// Create an instance. `c` should be a common ancestor state of `a` and `b`.
    /// 
    /// Operation is `O(A + B + X)` where `A` and `B` are the numbers of
    /// elements in states `a` and `b` respectively and `X` are the number of
    /// conflicts.
    pub fn new<'b>(a: &'b PartState<E>, b: &'b PartState<E>,
        c: &'b PartState<E>) -> TwoWayMerge<'b, E>
    {
        let mut v: Vec<(EltId, EltMerge<E>)> = Vec::new();
        // #0019: is using `collect()` for a HashMap efficient? Better to add a "clone_map" function to b?
        let mut map_b: HashMap<_,_> = b.elts_iter().collect();
        for (id, elt1) in a.elts_iter() {
            if let Some(elt2) = map_b.remove(&id) {
                // Have elt in states 1 and 2
                if elt1 != elt2 {
                    v.push((id, EltMerge::Fail));
                }
            } else {
                // Have elt in state 1 but not 2
                v.push((id, EltMerge::Fail));
            }
        }
        for (id, _) in map_b {
            // Have elt in state 2 but not 1
            v.push((id, EltMerge::Fail));
        }
        TwoWayMerge { a: a, b: b, c: c, v: v }
    }
    
    /// Run a solver over all still-ambiguous cases. This need not resolve all
    /// of them.
    /// 
    /// Operation is `O(X)`.
    pub fn solve<S>(&mut self, s: &S) where S: TwoWaySolver<E> {
        for &mut (id, ref mut result) in &mut self.v {
            if *result == EltMerge::Fail {
                *result = s.solve(self.a.get_rc(id).ok(), self.b.get_rc(id).ok(), self.c.get_rc(id).ok());
            }
        }
    }
    
    /// Run a solver. Same as `solve()` but consumes and returns self to allow
    /// chaining.
    pub fn solve_inline<S>(mut self, s: &S) -> Self where S: TwoWaySolver<E> {
        self.solve(s);
        self
    }
    
    /// Get the number of conflicts, solved or not.
    /// 
    /// Operation is `O(1)`.
    pub fn len(&self) -> usize { self.v.len() }
    
    /// Get the current resolution for conflict `i` (where `0 <= i < len()`).
    /// `EltMerge::Fail` means not-yet-solved. The element identifier is
    /// also given, though these are expected to be meaningless to the user.
    /// 
    /// Operation is `O(1)`.
    pub fn status(&self, i: usize) -> &(EltId, EltMerge<E>) {
        &self.v[i]
    }
    
    /// Run a solver on conflict `i` only (where `0 <= i < len()`).
    /// The trivial solvers, like `TwoWaySolveUseA`, can be used to set a
    /// result. Unlike `solve()`, this runs the solver even on already-decided
    /// cases.
    /// 
    /// Operation is `O(1)`.
    pub fn solve_one<S>(&mut self, i: usize, s: &S) where S: TwoWaySolver<E> {
        let id = self.v[i].0;
        self.v[i].1 = s.solve(self.a.get_rc(id).ok(), self.b.get_rc(id).ok(), self.c.get_rc(id).ok());
    }
    
    /// Get the number of unsolved conflicts.
    /// 
    /// Operation is `O(X)`.
    pub fn num_unsolved(&self) -> usize {
        self.v.iter().filter(|&&(_, ref result)| *result != EltMerge::Fail).count()
    }
    
    /// Check whether all conflicts have been resolved.
    /// 
    /// Operation is `O(X)`.
    pub fn is_solved(&self) -> bool {
        self.v.iter().all(|&(_, ref result)| *result != EltMerge::Fail)
    }
    
    /// Create a merge commit.
    /// 
    /// This succeeds if and only if `is_solved()` returns true.
    /// 
    /// Operation is `O(X)`.
    pub fn make_commit(self, mcm: &MakeCommitMeta) -> Option<Commit<E>> {
        // We build change-lists from the perspective of state1 and state2, then
        // pick whichever is smaller.
        let mut c1 = HashMap::new();
        let mut c2 = HashMap::new();
        // We calculate the new state-sums too.
        let mut sum1: Sum = self.a.statesum() ^ &self.a.metasum();
        let mut sum2: Sum = self.b.statesum() ^ &self.b.metasum();
        
        for (id, result) in self.v {
            let a = self.a.get_rc(id);
            let b = self.b.get_rc(id);
            match result {
                EltMerge::A => {
                    if let Ok(elt1) = a {
                        if let Ok(elt2) = b {
                            c2.insert(id, EltChange::replacement(elt1.clone()));
                            sum2.permute(&elt2.sum(id));
                            sum2.permute(&elt1.sum(id));
                        } else {
                            c2.insert(id, EltChange::insertion(elt1.clone()));
                            sum2.permute(&elt1.sum(id));
                        }
                    } else {
                        if let Ok(elt2) = b {
                            c2.insert(id, EltChange::deletion());
                            sum2.permute(&elt2.sum(id));
                        }
                    }
                },
                EltMerge::B => {
                    if let Ok(elt1) = a {
                        if let Ok(elt2) = b {
                            c1.insert(id, EltChange::replacement(elt2.clone()));
                            sum1.permute(&elt1.sum(id));
                            sum1.permute(&elt2.sum(id));
                        } else {
                            c1.insert(id, EltChange::deletion());
                            sum1.permute(&elt1.sum(id));
                        }
                    } else {
                        if let Ok(elt2) = b {
                            c1.insert(id, EltChange::insertion(elt2.clone()));
                            sum1.permute(&elt2.sum(id));
                        }
                    }
                },
                EltMerge::Value(elt) => {
                    if let Ok(elt1) = a {
                        if *elt1 != elt {
                            sum1.permute(&elt1.sum(id));
                            sum1.permute(&elt.sum(id));
                            c1.insert(id, EltChange::replacement(elt.clone()));
                        }
                    } else {
                        sum1.permute(&elt.sum(id));
                        c1.insert(id, EltChange::insertion(elt.clone()));
                    }
                    if let Ok(elt2) = b {
                        if *elt2 != elt {
                            sum2.permute(&elt2.sum(id));
                            sum2.permute(&elt.sum(id));
                            c2.insert(id, EltChange::replacement(elt));
                        }
                    } else {
                        sum2.permute(&elt.sum(id));
                        c2.insert(id, EltChange::insertion(elt));
                    }
                },
                EltMerge::Delete => {
                    if let Ok(elt1) = a {
                        c1.insert(id, EltChange::deletion());
                        sum1.permute(&elt1.sum(id));
                    }
                    if let Ok(elt2) = b {
                        c2.insert(id, EltChange::deletion());
                        sum2.permute(&elt2.sum(id));
                    }
                },
                EltMerge::Rename => {
                    if let Ok(elt1) = a {
                        if let Ok(elt2) = b {
                            let new_id = match self.a.gen_id_binary(self.b) {
                                Ok(id) => id,
                                Err(_) => { /*#0017: warn about failure*/
                                    return None;
                                }
                            };
                            
                            c1.insert(new_id, EltChange::insertion(elt2.clone()));
                            sum1.permute(&elt2.sum(id));
                            c2.insert(new_id, EltChange::insertion(elt1.clone()));
                            sum2.permute(&elt1.sum(id));
                        } else {
                            c2.insert(id, EltChange::insertion(elt1.clone()));
                            sum2.permute(&elt1.sum(id));
                        }
                    } else {
                        if let Ok(elt2) = b {
                            c1.insert(id, EltChange::insertion(elt2.clone()));
                            sum1.permute(&elt2.sum(id));
                        }
                    }
                },
                EltMerge::Fail => {
                    return None;
                }
            }
        }
        assert_eq!(sum1, sum2); // sums must be equal
        
        let (first, second, changes) = if c1.len() < c2.len() {
            trace!("Created merge from first parent: {}", self.a.statesum());
            (self.a, self.b, c1)
        } else {
            trace!("Created merge from second parent: {}", self.b.statesum());
            (self.b, self.a, c2)
        };
        
        let parents = vec![(first.statesum(), first.meta()), (second.statesum(), second.meta())];
        let meta = CommitMeta::new_parents(parents, mcm);
        
        let parents = vec![first.statesum().clone(), second.statesum().clone()];
        let statesum = &sum1 ^ &Sum::state_meta_sum(&parents, &meta);
        
        Some(Commit::new_explicit(statesum, parents, changes, meta))
    }
    
    /* One could in theory just go through elements once, like this. This is
     * more efficient, but less flexible.
    /// Try to merge using the provided solver.
    /// 
    /// Should the solver return `EltMerge::Fail` in any case, the merge fails and this
    /// function returns `None`. Otherwise, this function returns a new commit.
    /// 
    /// This goes through all elements in states `a` and/or `b`, and refers to
    /// the solver whenever the elements are not equal.
    pub fn merge<S>(s: &S) -> Option<Commit> where S: TwoWaySolver {
        // We build change-lists from the perspective of state1 and state2, then
        // pick whichever is smaller.
        let mut c1 = HashMap::new();
        let mut c2 = HashMap::new();
        
        let map2 = state2.map().clone();
        for (id, elt1) in state1.map() {
            if let Some(elt2) = map2.remove(id) {
                // Have elt in states 1 and 2
                if elt1 != elt2 {
                    let elt3 = common.get_elt(id);
                    match s.solve(Some(elt1), Some(elt2), elt3) {
                        EltMerge::A => {
                            c2.insert(id, EltChange::replacement(elt1));
                        },
                        EltMerge::B => {
                            c1.insert(id, EltChange::replacement(elt2));
                        },
                        EltMerge::Other(elt) {
                            if elt != elt1 {
                                c1.insert(id, EltChange::replacement(elt));
                            }
                            if elt != elt2 {
                                c2.insert(id, EltChange::replacement(elt));
                            }
                        },
                        EltMerge::Delete {
                            c1.insert(id, EltChange::deletion());
                            c2.insert(id, EltChange::deletion());
                        },
                        EltMerge::Rename {
                            let new_id = ...;
                            c1.insert(new_id, EltChange::insertion(elt2));
                            c2.insert(new_id, EltChange::change_id(new_id));
                            c2.insert(id, EltChange::insertion(elt1));
                        },
                        EltMerge::Fail {
                            return None;
                        }
                    };
                }
            } else {
                // Have elt in state 1 but not 2
                let elt3 = common.get_elt(id);
                match s.solve(Some(elt1), None, elt3) {
                    EltMerge::A | EltMerge::Rename => {
                        c2.insert(id, EltChange::insertion(elt1));
                    },
                    EltMerge::B | EltMerge::Delete => {
                        c1.insert(id, EltChange::deletion());
                    },
                    EltMerge::Other(elt) {
                        if elt != elt1 {
                            c1.insert(id, EltChange::replacement(elt));
                        }
                        c2.insert(id, EltChange::insertion(elt));
                    },
                    EltMerge::Fail {
                        return None;
                    }
                };
            }
        }
        for (id, elt2) in map2 {
            // Have elt in state 2 but not 1
            let elt3 = common.get_elt(id);
            match s.solve(None, Some(elt2), elt3) {
                EltMerge::A | EltMerge::Delete => {
                    c2.insert(id, EltChange::deletion());
                },
                EltMerge::B EltMerge::Rename => {
                    c1.insert(id, EltChange::insertion(elt2));
                },
                EltMerge::Other(e) {
                    c1.insert(id, EltChange::insertion(e));
                    if elt != elt2 {
                        c2.insert(id, EltChange::replacement(elt));
                    }
                },
                EltMerge::Fail {
                    return None;
                }
            };
        }
        
        ! need to calculate sum1 and sum2 !
        Some(if c1.len() < c2.len() {
            Commit::new(sum1, vec![state1.statesum().clone(), state2.statesum().clone()], c1)
        } else {
            Commit::new(sum2, vec![state2.statesum().clone(), state1.statesum().clone()], c2)
        })
    }
    */
}

/// Return type of a by-element merge solver.
/// 
/// Note that there is no direct way to specify the ancestor value, but this
/// can be replicated via `EltMerge::Value(...)` and `EltMerge::Delete`. This
/// significantly simplifies code in `TwoWayMerge::merge()`.
#[derive(PartialEq, Eq)]
pub enum EltMerge<E: Element> {
    /// Use the value from first state
    A,
    /// Use the value from the second state
    B,
    /// Use a custom value (specified in full)
    Value(Rc<E>),
    /// Remove the element
    Delete,
    /// Rename one element and include both; where only one element is present
    /// that element is used in both.
    Rename,
    /// Give up
    Fail,
}

/// Implementations solve two-way merges on an element-by-element basis.
pub trait TwoWaySolver<E: Element> {
    /// This function should take possibly-present elements from states A, B
    /// and common ancestor state C, which all have the same identifier, and
    /// return an `EltMerge` object.
    fn solve<'a>(&self, a: Option<&'a Rc<E>>, b: Option<&'a Rc<E>>,
        c: Option<&'a Rc<E>>) -> EltMerge<E>;
}

/// Implementation of `TwoWaySolver` which always selects state A.
pub struct TwoWaySolveUseA<E: Element>{
    p: PhantomData<E>
}
impl<E: Element> TwoWaySolveUseA<E> {
    /// Create an instance (requires no parameters)
    pub fn new() -> Self {
        TwoWaySolveUseA { p: PhantomData }
    }
}
impl<E: Element> TwoWaySolver<E> for TwoWaySolveUseA<E> {
    fn solve(&self, _: Option<&Rc<E>>, _: Option<&Rc<E>>,
        _: Option<&Rc<E>>) -> EltMerge<E>
    {
        EltMerge::A
    }
}
/// Implementation of `TwoWaySolver` which always selects state B.
pub struct TwoWaySolveUseB<E: Element>{
    p: PhantomData<E>
}
impl<E: Element> TwoWaySolveUseB<E> {
    /// Create an instance (requires no parameters)
    pub fn new() -> Self {
        TwoWaySolveUseB { p: PhantomData }
    }
}
impl<E: Element> TwoWaySolver<E> for TwoWaySolveUseB<E> {
    fn solve(&self, _: Option<&Rc<E>>, _: Option<&Rc<E>>,
        _: Option<&Rc<E>>) -> EltMerge<E>
    {
        EltMerge::B
    }
}
/// Implementation of `TwoWaySolver` which always selects state C.
pub struct TwoWaySolveUseC<E: Element>{
    p: PhantomData<E>
}
impl<E: Element> TwoWaySolveUseC<E> {
    /// Create an instance (requires no parameters)
    pub fn new() -> Self {
        TwoWaySolveUseC { p: PhantomData }
    }
}
impl<E: Element> TwoWaySolver<E> for TwoWaySolveUseC<E> {
    fn solve(&self, _: Option<&Rc<E>>, _: Option<&Rc<E>>,
        c: Option<&Rc<E>>) -> EltMerge<E>
    {
        match c {
            Some(elt) => EltMerge::Value((*elt).clone()),
            None => EltMerge::Delete,
        }
    }
}
/// Implementation of `TwoWaySolver` which always gives up.
pub struct TwoWaySolveFail<E: Element>{
    p: PhantomData<E>
}
impl<E: Element> TwoWaySolveFail<E> {
    /// Create an instance (requires no parameters)
    pub fn new() -> Self {
        TwoWaySolveFail { p: PhantomData }
    }
}
impl<E: Element> TwoWaySolver<E> for TwoWaySolveFail<E> {
    fn solve(&self, _: Option<&Rc<E>>, _: Option<&Rc<E>>,
        _: Option<&Rc<E>>) -> EltMerge<E>
    {
        EltMerge::Fail
    }
}

/// Chains two solvers. Calls the second if and only if the first returns
/// `EltMerge::Fail`.
pub struct TwoWaySolverChain<'a, E: Element,
    S: TwoWaySolver<E>+'a, T: TwoWaySolver<E>+'a>
{
    s: &'a S, t: &'a T,
    p: PhantomData<E>
}
impl<'a, E: Element, S: TwoWaySolver<E>+'a, T: TwoWaySolver<E>+'a>
    TwoWaySolverChain<'a, E, S, T>
{
    /// Create an instance, based on two other solvers
    pub fn new(s: &'a S, t: &'a T) -> TwoWaySolverChain<'a, E, S, T> {
        TwoWaySolverChain{ s: s, t: t, p: PhantomData }
    }
}
impl<'a, E: Element, S: TwoWaySolver<E>+'a, T: TwoWaySolver<E>+'a> TwoWaySolver<E>
    for TwoWaySolverChain<'a, E, S, T>
{
    fn solve(&self, a: Option<&Rc<E>>, b: Option<&Rc<E>>,
        c: Option<&Rc<E>>) -> EltMerge<E>
    {
        let result = self.s.solve(a, b, c);
        if result != EltMerge::Fail {
            result
        } else {
            self.t.solve(a, b, c)
        }
    }
}

/// Solver which tries to make sensible choices by comparing to the common
/// ancestor. In brief, if one state has element equal to that in the ancestor
/// (or neither has the element in question), the element from the other state
/// (or its absense) will be used. In other cases, this returns `EltMerge::Fail`.
/// 
/// (This isn't quite right, e.g. if two branches perform the same change
/// independently, then one reverts, and then a merge is carried out, the
/// merge will ignore the revert. Git and any other "3-way-merge" algorithms
/// have the same defect.)
pub struct AncestorSolver2W<E: Element>{
    p: PhantomData<E>
}
impl<E: Element> AncestorSolver2W<E> {
    /// Create an instance (requires no parameters)
    pub fn new() -> Self {
        AncestorSolver2W { p: PhantomData }
    }
}
impl<E: Element> TwoWaySolver<E> for AncestorSolver2W<E> {
    fn solve<'a>(&self, a: Option<&'a Rc<E>>, b: Option<&'a Rc<E>>,
        c: Option<&'a Rc<E>>) -> EltMerge<E>
    {
        // Assumption: a != b
        if a == c {
            return EltMerge::B;
        }
        if b == c {
            return EltMerge::A;
        }
        EltMerge::Fail
    }
}

/// Solver which handles the case where there is no common ancestor element by
/// renaming (or in the case that either `a` or `b` is `None`, choosing the
/// other).
pub struct RenamingSolver2W<E: Element>{
    p: PhantomData<E>
}
impl<E: Element> RenamingSolver2W<E> {
    /// Create an instance (requires no parameters)
    pub fn new() -> Self {
        RenamingSolver2W { p: PhantomData }
    }
}
impl<E: Element> TwoWaySolver<E> for RenamingSolver2W<E> {
    fn solve(&self, _: Option<&Rc<E>>, _: Option<&Rc<E>>,
        c: Option<&Rc<E>>) -> EltMerge<E>
    {
        if c == None {
            EltMerge::Rename
        } else {
            EltMerge::Fail
        }
    }
}
