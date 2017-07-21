/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! "Scrap" code which may later find a use.


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


pub trait XControl {
    /// Iterate through available `Property` functions.
    /// 
    /// This method allows discovery of properties available for partitioning. Partitioning
    /// prioritises use of properties in the order listed by this function.
    /// 
    /// All properties accessible through this method should be accessible through `prop_fn` too.
    /// The reverse is not required, e.g. properties retained for backwards compatibility should
    /// remain available through `prop_fn` but need not be listed by `props_iter`.
    /// 
    /// The default implementation returns `std::iter::empty()`.
    fn props_iter(&self) -> Box<Iterator<Item=Property<Self::Element>>> {
        Box::new(iter::empty())
    }
    
    /// Get a property function by identifier, if available.
    /// 
    /// This method allows access to properties already used for partitioning. Any properties used
    /// must remain available with the same `PropId`.
    /// 
    /// Default implementation returns `None`.
    fn prop_fn(&self, _id: PropId) -> Option<Property<Self::Element>> {
        None
    }
}
