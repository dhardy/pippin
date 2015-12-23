//! Base type of elements stored in Pippin repositories

use std::fmt;
use std::rc::Rc;
use std::clone::Clone;
// use vec_map::VecMap;

use super::sum::Sum;


/// Holds an element's data in memory
// TODO: replace with a trait and user-defined implementation?
// TODO: put data at end and make this an unsized type?
#[derive(PartialEq,Eq)]
pub struct Element {
    /// Element checksum, used in calculating state sums
    sum: Sum,
    /// Element data
    data: Rc<Vec<u8>>,
//     /// A list of cached classifier values (since classifiers are always
//     /// identified by a small non-negative integer, a vector makes a suitable
//     /// mapping structure). The special value of 0 indicates either that the
//     /// value was never cached or that the classifier function 'failed'.
//     /// 
//     /// This is usually set when elements are loaded or inserted into a
//     /// partition.
//     csfs: Vec<u32>,
}

impl Element {
    /// Create, specifying data as a Vec and sum
    pub fn new(data: Vec<u8>, sum: Sum) -> Element {
        Element {
            data: Rc::new(data),
            sum: sum,
//             csfs: Vec::new(),
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
    
    /// Get a reference to the checksum
    pub fn sum(&self) -> &Sum { &self.sum }
    
    /// Get a reference to the data (raw)
    pub fn data(&self) -> &[u8] { &*self.data }
    
    /// Get the length of the data
    pub fn data_len(&self) -> usize {
        self.data.len()
    }
    
//     /// Get the cached value of a classifier, or 0 if no value was cached.
//     pub fn cached_classifier(&self, classifier: u32) -> u32 {
//         if classifier < self.csfs.len() {
//             self.csfs[classifier]
//         } else { 0 }
//     }
    
    //TODO: access only within library
//     /// For use only within the library.
//     fn cache_classifiers(&mut self, classifiers: &VecMap<Classifier>) {
//         self.csfs.reset();
//         let end = classifiers.iter().filter(|c| c.use_caching()).next_back().map(|i| i+1).unwrap_or(0);
//         self.csfs.resize(end);
//         for i in 0..end {
//             self.csfs[i] = classifiers[i].map(|c| c.classify(self)).unwrap_or(0);
//         }
//     }
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
