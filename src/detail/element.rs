//! Base type of elements stored in Pippin repositories

use std::fmt;
use std::rc::Rc;
use std::clone::Clone;
// use vec_map::VecMap;

use super::sum::Sum;


/// Holds an element's data in memory
// #0014: replace with a trait and user-defined implementation?
#[derive(PartialEq,Eq)]
pub struct Element {
    /// Element details are reference counted to make cloning cheap
    r: Rc<EltData>,
}
#[derive(PartialEq,Eq)]
struct EltData {
    /// Element checksum, used in calculating state sums
    sum: Sum,
    /// Element data
    data: Vec<u8>,
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
        Element { r: Rc::new(EltData{
                data: data,
                sum: sum,
    //             csfs: Vec::new(),
        }) }
    }
    /// Create, from a Vec of the data
    pub fn from_vec(data: Vec<u8>) -> Element {
        let sum = Sum::calculate(&data[..]);
        Element { r: Rc::new(EltData{
            data: data,
            sum: sum
        }) }
    }
    /// Create from an str. Note that this allocates currently (it is only used
    /// for testing, thus not optimised).
    pub fn from_str(data: &str) -> Element {
        Element { r: Rc::new(EltData{
            data: data.as_bytes().to_vec(),
            sum: Sum::calculate(data.as_bytes())
        }) }
    }
    
    /// Get a reference to the checksum
    pub fn sum(&self) -> &Sum { &self.r.sum }
    
    /// Get a reference to the data (raw)
    pub fn data(&self) -> &[u8] { &self.r.data }
    
    /// Get the length of the data
    pub fn data_len(&self) -> usize {
        self.r.data.len()
    }
    
//     /// Get the cached value of a classifier, or 0 if no value was cached.
//     pub fn cached_classifier(&self, classifier: u32) -> u32 {
//         if classifier < self.r.csfs.len() {
//             self.r.csfs[classifier]
//         } else { 0 }
//     }
    
    //TODO: access only within library
//     /// For use only within the library.
//     fn cache_classifiers(&mut self, classifiers: &VecMap<Classifier>) {
//         self.r.csfs.reset();
//         let end = classifiers.iter().filter(|c| c.use_caching()).next_back().map(|i| i+1).unwrap_or(0);
//         self.r.csfs.resize(end);
//         for i in 0..end {
//             self.r.csfs[i] = classifiers[i].map(|c| c.classify(self)).unwrap_or(0);
//         }
//     }
}

impl fmt::Debug for Element {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "element (len {})", self.r.data.len())
    }
}
impl Clone for Element {
    /// Elements are Copy-On-Write, so cloning is cheap
    fn clone(&self) -> Element {
        Element { r: self.r.clone() }
    }
}
