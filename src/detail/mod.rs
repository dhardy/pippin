//! In-memory representations of Pippin data

use std::fmt;
use std::rc::Rc;
use std::collections::HashMap;
use std::clone::Clone;

pub use self::readwrite::{FileHeader, read_head, write_head, validate_repo_name};
pub use self::readwrite::{read_snapshot, write_snapshot};
pub use self::states::{RepoState};
pub use self::sum::Sum;

mod readwrite;
mod sum;
mod states;


/// Holds an element's data in memory
// TODO: replace with a trait and user-defined implementation?
// TODO: put data at end and make this an unsized type?
#[derive(PartialEq,Eq)]
pub struct Element {
    /// Element data
    data: Rc<Vec<u8>>,
    /// Element checksum, used in calculating state sums
    sum: Sum,
}

impl Element {
    /// Create, specifying data as a Vec and sum
    pub fn new(data: Vec<u8>, sum: Sum) -> Element {
        Element {
            data: Rc::new(data),
            sum: sum
        }
    }
    /// Create, from a Vec of the data
    pub fn from_vec(data: Vec<u8>) -> Element {
        let sum = Sum::calculate(&data[..]);
        Element {
            data: Rc::new(data),
            sum: sum
        }
    }
    /// Create from an str. Note that this allocates currently (it is only used
    /// for testing, thus not optimised).
    pub fn from_str(data: &str) -> Element {
        Element {
            data: Rc::new(data.as_bytes().to_vec()),
            sum: Sum::calculate(data.as_bytes())
        }
    }
    /// Get a reference to the data (raw)
    pub fn data(&self) -> &[u8] { &*self.data }
    
    /// Get the length of the data
    pub fn data_len(&self) -> usize {
        self.data.len()
    }
}

impl fmt::Debug for Element {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "element (len {})", self.data.len())
    }
}
impl Clone for Element {
    /// Elements are Copy-On-Write, so cloning is cheap
    fn clone(&self) -> Self {
        Element { data: self.data.clone(), sum: self.sum }
    }
}
