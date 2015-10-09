//! In-memory representations of Pippin data

use std::fmt;
use std::collections::HashMap;

pub use self::sum::Sum;

mod sum;


/// Holds an element's data in memory
// TODO: replace with a trait and user-defined implementation?
#[derive(PartialEq,Eq)]
pub struct Element {
    //TODO: make this private, but keep it accessible to the readwrite module?
    pub data: Vec<u8>,
    /// Element checksum, used in calculating state sums
    pub sum: Sum,
}

impl Element {
    /// Get a reference to the data (raw)
    pub fn data(&self) -> &[u8] { &self.data }
}

impl fmt::Debug for Element {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "element (len {})", self.data.len())
    }
}


/// A commit: a set of changes
pub struct Commit {
    /// Expected resultant state sum; doubles as an ID
    pub statesum: Sum,
    /// State sum (ID) of parent commit/snapshot
    pub parent: Sum,
    /// Time when this commit was made (TODO)
    pub timestamp: (),
    /// Per-element changes
    pub changes: HashMap<u64, EltChange>
}

/// Per-element changes
pub enum EltChange {
    /// Element was deleted
    Deletion,
    /// Element was added (full data)
    Insertion(Element),
    /// Element was replaced (full data)
    Replacement(Element),
    //TODO: patches (?)
}
