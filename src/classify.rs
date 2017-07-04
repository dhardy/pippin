/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Trait for user-defined classification

// use std::marker::PhantomData;
use std::collections::BTreeMap;
use std::u32;

use elt::{Element, PartId};
use error::{Result, ClassifyError, OtherError};
use repo::RepoControl;


/// Identifier for a property function. Identifiers should not change or be reused.
/// 
/// To see a readable identifier in files this should be a big-endian sequence of four ASCII bytes.
pub type PropId = u32;

/// Domain for property functions.
pub type PropDomain = u32;

/// A user-defined function mapping from an element to a value, used for partitioning and search.
/// 
/// This could be anything from a property with intuitive meaning (e.g. the element's size or
/// time of creation) to something with no intuitive meaning like a hash function. It must be
/// deterministic and reproducible.
/// 
/// The result must be an integer, but there are no requirements on distribution within the
/// representable range.
pub struct Property<E: Element> {
    /// Unique identifier. (Should never change; this number is stored in data files.)
    pub id: PropId,
    /// A pointer to the actual function
    pub f: fn(&E) -> PropDomain,
}

/// Classification type stored in headers
pub type ClassificationRanges = Vec<(PropId, u32, u32)>;

/// Each partition has a classification, defining which elements it accepts.
#[derive(PartialEq, Eq, Debug, Clone)]
pub struct Classification {
    // Each property used has one entry in the `BTreeMap`. The internal `Vec` is a list of
    // inclusive ranges within the property domain considered within this classification.
    // 
    // An element matches this classification if for all properties used, the
    // value is included in (at least) one of the associated ranges.
    // 
    // Special case: if any `PropId` is listed with an empty vector, this classification matches no
    // elements. In this case the `PropId` does not need to match any `Property`.
    rules: BTreeMap<PropId, Vec<(PropDomain, PropDomain)>>,
}

impl Classification {
    /// Create a new classification matching all elements
    pub fn all() -> Self {
        Classification { rules: Default::default() }
    }
    
    /// Create a new classification matching nothing
    pub fn none() -> Self {
        let mut csf = Classification::all();
        csf.rules.insert(0, vec![]);
        csf
    }
    
    /// Split an existing classification into two. `prop` is the property used to differentiate
    /// between the two new classifications, and `pivot` is the largest value of this property
    /// accepted in the first classification.
    pub fn split(&self, prop: PropId, pivot: PropDomain) -> Result<(Classification, Classification)> {
        let mut rules = self.rules.clone();
        let opt_ranges = rules.remove(&prop);
        let (r1, r2) = if let Some(ranges) = opt_ranges {
            let mut pos = None;
            for (i, r) in ranges.iter().enumerate() {
                if r.0 <= pivot && r.1 >= pivot {
                    if r.1 == pivot {
                        return OtherError::err("Classification::split: one half would be empty");
                    }
                    pos = Some(i);
                    break;
                }
            }
            let pos = if let Some(p) = pos {
                p
            } else {
                return OtherError::err("Classification::split: pivot not in source classification");
            };
            let (mut r1, mut r2) = (ranges.clone(), ranges);
            r1[pos].1 = pivot;
            r2[pos].0 = pivot + 1;
            (r1, r2)
        } else {
            (vec![(0, pivot)], vec![(pivot + 1, u32::MAX)])
        };
        let (mut rules1, mut rules2) = (rules.clone(), rules);
        rules1.insert(prop, r1);
        rules2.insert(prop, r2);
        Ok((Classification{ rules: rules1 }, Classification { rules: rules2 }))
    }
    
    /// Create from header ranges
    pub fn from_ranges(ranges: &ClassificationRanges) -> Classification {
        let mut combined = BTreeMap::new();
        for &(name, min, max) in ranges {
            combined.entry(name).or_insert(vec![]).push((min, max));
        }
        Classification {
            rules: combined,
        }
    }
    
    /// Export format used for serialisation
    pub fn make_ranges(&self) -> ClassificationRanges {
        let mut result = Vec::with_capacity(self.rules.len());
        for (name, ranges) in &self.rules {
            for &(min, max) in ranges {
                result.push((*name, min, max));
            }
        }
        result
    }
    
    /// Checks whether an element matches this classification.
    /// 
    /// For frequent uses, building a checker with `make_checker` and reusing it should be faster.
    pub fn matches_elt<R: RepoControl>(&self, elt: &R::Element, control: &R) -> Result<bool, ClassifyError> {
        'outer: for (cfr, ranges) in &self.rules {
            let v = (control.prop_fn(*cfr).ok_or(ClassifyError::UnknownProperty)?.f)(elt);
            for &(min,max) in ranges {
                if min <= v && v <= max {
                    continue 'outer;
                }
            }
            // property does not match any range
            return Ok(false);
        }
        // all properties match some range
        Ok(true)
    }
    
    /// Build a checker for this classification
    pub fn make_checker<R: RepoControl>(&self, control: &R) ->
            Result<CsfChecker<R::Element>, ClassifyError>
    {
        // #0019: is it worth reordering rules? If nearly all elements match the
        // first rule it might be better to test that last, but how do we know?
        let mut rules = Vec::with_capacity(self.rules.len());
        for (cfr, ranges) in &self.rules {
            rules.push((control.prop_fn(*cfr).ok_or(ClassifyError::UnknownProperty)?.f, ranges.clone()));
        }
        Ok(CsfChecker {
            rules: rules
        })
    }
}

/// Tool for testing whether an element fits the given classification.
pub struct CsfChecker<E: Element> {
    rules: Vec<(fn(&E) -> PropDomain, Vec<(PropDomain, PropDomain)>)>,
}
impl<E: Element> CsfChecker<E> {
    /// Checks whether an element matches this classification.
    pub fn matches(&self, elt: &E) -> bool {
        'outer: for &(ref prop, ref ranges) in &self.rules {
            let v = (prop)(elt);
            for &(min, max) in ranges {
                if min <= v && v <= max {
                    continue 'outer;
                }
            }
            return false;   // no range matches
        }
        true    // each property matches a range
    }
}
impl<E: Element> Clone for CsfChecker<E> {
    fn clone(&self) -> Self {
        CsfChecker {
            rules: self.rules.iter().map(|x| (x.0, x.1.clone())).collect()
        }
    }
}

/// Tool for finding a matching classification given an element
pub struct CsfFinder<E: Element> {
    checkers: Vec<(PartId, CsfChecker<E>)>,
}
impl<E: Element> CsfFinder<E> {
    /// Create a new finder. Populate classifications with `add_csf`.
    pub fn new() -> Self {
        CsfFinder { checkers: Vec::new() }
    }
    
    /// Add a new classification.
    // TODO: can we check for overlaps here?
    pub fn add_csf<R: RepoControl<Element = E>>(&mut self, part_id: PartId, csf: &Classification,
            control: &R) -> Result<(), ClassifyError>
    {
        let checker = csf.make_checker(control)?;
        self.checkers.push((part_id, checker));
        Ok(())
    }
    
    /// Remove a classification
    /// 
    /// Returns true if and only if a classification was removed
    pub fn remove_csf(&mut self, part_id: PartId) -> bool {
        if let Some(index) = self.checkers.iter().position(|x| x.0 == part_id) {
            self.checkers.remove(index);
            true
        } else {
            false
        }
    }
    
    /// Looks for a matching classification.
    /// 
    /// Returns `None` if and only if no known classifications match.
    pub fn find(&self, elt: &E) -> Option<PartId> {
        // TODO: more efficient implementation. We should only need to test each property of the
        // element once if we test by property first instead of classification. We could also
        // optimise the order of properties over time by predictive power.
        for &(part_id, ref checker) in &self.checkers {
            if checker.matches(elt) {
                return Some(part_id);
            }
        }
        None
    }
}
impl<E: Element> Clone for CsfFinder<E> {
    fn clone(&self) -> Self {
        CsfFinder {
            checkers: self.checkers.clone()
        }
    }
}
impl<E: Element> Default for CsfFinder<E> {
    fn default() -> Self {
        CsfFinder::new()
    }
}
