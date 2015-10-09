//! In-memory representations of Pippin data

use std::fmt;
use std::collections::HashMap;

pub use self::readwrite::{FileHeader, read_head, write_head, validate_repo_name};
pub use self::readwrite::{read_snapshot, write_snapshot};

pub use self::sum::Sum;

mod readwrite;
mod sum;


/// Holds an element's data in memory
// TODO: replace with a trait and user-defined implementation?
#[derive(PartialEq,Eq)]
pub struct Element {
    /// Element data TODO make private
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
    statesum: Sum,
    /// State sum (ID) of parent commit/snapshot
    parent: Sum,
    /// Time when this commit was made (TODO)
    timestamp: (),
    /// Per-element changes
    changes: HashMap<u64, EltChange>
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
