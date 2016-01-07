//! Base type of elements stored in Pippin repositories

use std::fmt::Debug;
use std::rc::Rc;
use std::clone::Clone;
use std::io::{/*Read,*/ Write};
use std::str::from_utf8;
use std::ops::Deref;
// use vec_map::VecMap;

use super::sum::Sum;
use ::error::{Result};


/// Whatever element type the user wishes to store must implement this trait.
/// 
/// There is a default implementation for `String`.
pub trait ElementT where Self: Sized+PartialEq+Debug {
    // TODO: provide a choice of how to implement IO using a const?
    // associated constants are experimental (see issue #29646)
    // 
//     /// If this is set true, the `read_buf` and `write_buf` functions must be
//     /// implemented. These are easier to use but potentially less efficient. If
//     /// this is set false, then the `read`, `write` and `write_len` functions
//     /// must be implemented.
//     /// 
//     /// The other functions may simply be empty or panic since they will not be
//     /// used.
//     const use_buf_io: bool;
    
    /// Write a serialisation of the element data out to the given writer.
    /// 
    /// The given writer points to a dynamically allocated buffer so that
    /// length can be determined before the contents are finally written out
    /// (see `use_buf_io`).
    fn write_buf(&self, writer: &mut Write) -> Result<()>;
    /// Deserialise the given data into a new element.
    fn read_buf(buf: &[u8]) -> Result<Self>;
    
//     /// Get the length of data which will be written out by `write()`. This
//     /// *must* be correct!
//     fn write_len(&self) -> Result<usize>;
//     /// Write out data to a writer. Length *must* be that specified by
//     /// `write_len()`!
//     fn write<W: Write>(&self, writer: W) -> Result<()>;
//     /// Read from a data stream. The implementation *must* read `len` bytes!
//     fn read<R: Read>(reader: R) -> Result<Self>;
    
    /// This can either return a copy of an internally stored sum or calculate
    /// one on the fly. It is used when inserting, removing or replacing an
    /// element in a state, and when merging states where the element differs.
    /// 
    /// Warning: this implementation panics if `write_buf` has an error!
    fn get_sum(&self) -> Sum {
        let mut buf = Vec::new();
        self.write_buf(&mut &mut buf).expect("write_buf does not fail in get_sum");
        Sum::calculate(&buf)
    }
}

impl ElementT for String {
    fn write_buf(&self, writer: &mut Write) -> Result<()> {
        try!(writer.write(self.as_bytes()));
        Ok(())
    }
    fn read_buf(buf: &[u8]) -> Result<Self> {
        let s = try!(from_utf8(buf));
        Ok(s.to_string())
    }
}


/// Holds an element's data in memory.
/// 
/// This is a wrapper around a user-defined type implementing the trait
/// `ElementT`, such that instances are read-only and reference counted (to
/// support the database's copy-on-write policy).
#[derive(PartialEq, Debug)]
pub struct Element<E: ElementT> {
    /// Element details are reference counted to make cloning cheap
    r: Rc<E>,
}

impl<E: ElementT> Element<E> {
    /// Create, consuming an element of user-defined type
    pub fn new(elt: E) -> Element<E> {
        Element { r: Rc::new(elt) }
    }
    
    /// Convenience function to create from a Vec of the data
    /// 
    /// TODO: remove?
    pub fn from_vec(data: Vec<u8>) -> Result<Element<E>> {
        let user_elt = try!(E::read_buf(&data));
        Ok(Element { r: Rc::new(user_elt) })
    }
    
    /// Get a reference to the checksum
    pub fn sum(&self) -> Sum { self.r.get_sum() }
    
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

impl<E: ElementT> Clone for Element<E> {
    /// Elements are Copy-On-Write, so cloning is cheap
    fn clone(&self) -> Element<E> {
        Element { r: self.r.clone() }
    }
}

impl<E: ElementT> Deref for Element<E> {
    type Target = E;

    fn deref(&self) -> &E {
        &*self.r
    }
}
