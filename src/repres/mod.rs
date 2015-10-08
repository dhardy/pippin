//! In-memory representations of Pippin data

use std::fmt;

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
