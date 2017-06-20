/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Trait for user-defined classification

// use std::marker::PhantomData;
use std::collections::BTreeMap;

use elt::Element;
use error::{Result, ClassifyError};
use repo::RepoControl;


/// Identifier for a property function. Identifiers should not change or be reused.
/// 
/// To see a readable identifier in files this should be a big-endian sequence of four ASCII bytes.
pub type PropId = u32;

/// Domain for property functions.
pub type PropDomain = u32;

/// A user-defined function mapping from an element to a value, used for partitioning and search.
/// 
/// Notation: an object implementing this trait is considered a *function*, in the mathematical
/// sense, if not in terms of the language itself.
/// 
/// This could be anything from a property with intuitive meaning (e.g. the element's size or
/// time of creation) to something with no intuitive meaning like a hash function. It must be
/// deterministic and reproducible.
pub trait Property {
    /// The element type to be classified.
    type Element: Element;
    
    /// The property function. The result must be an integer, but there are no requirements on
    /// distribution within the representable range.
    fn p(&self, elt: &Self::Element) -> PropDomain;
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
    rules: BTreeMap<PropId, Vec<(u32, u32)>>,
}

impl Classification {
    /// Create a new classification matching all elements
    pub fn all() -> Self {
        Classification { rules: Default::default() }
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
    pub fn matches_elt<R: RepoControl>(&self, elt: &R::Element, control: &R) -> Result<bool, ClassifyError> {
        'outer: for (cfr, ranges) in &self.rules {
            let v = control.prop_fn(*cfr).ok_or(ClassifyError::UnknownProperty)?.p(elt);
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
}
