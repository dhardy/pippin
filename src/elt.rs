/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Base type of elements stored in Pippin repositories

use std::fmt;
use std::fmt::Debug;
use std::io::{/*Read,*/ Write};
use std::str::from_utf8;

use rand::random;

use sum::Sum;
use error::Result;

/// An element identifier.
/// 
/// TODO: should we reserve some bits or allow custom-length user prefixes?
/// Do we need 64-bit identifiers? Alternatively, once we have properties and
/// lookup tables, *this* identifier might not be needed at all.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub struct EltId {
    id: u64,
}
impl EltId {
    /// Get the next element identifier, wrapping if necessary.
    pub fn next_elt(self) -> EltId {
        EltId { id: self.id.wrapping_add(1) }
    }
    /// Get a random element identifier
    pub fn random() -> EltId {
        // #0049: configurable source of randomness?
        EltId { id: random::<u64>() }
    }
}
impl From<u64> for EltId {
    fn from(n: u64) -> EltId {
        EltId { id: n }
    }
}
impl Into<u64> for EltId {
    fn into(self) -> u64 {
        self.id
    }
}
impl fmt::Display for EltId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.id)
    }
}

/// Whatever element type the user wishes to store must implement this trait.
/// 
/// ### Read-only
/// 
/// Elements are not usually modifiable. The database only allows elements to
/// be updated by "replacing" them in the DB. If you attempt to get around this
/// by use of `std::cell` or similar, your changes will not be saved and may
/// affect historical states of the repository stored in memory, possibly even
/// affecting commit creation. Not recommended.
/// 
/// ### Equivalence
/// 
/// It must be possible to check the equivalence of two elements. This is used
/// to generate change-sets and handle merges.
/// 
/// ### Serialisation
/// 
/// Elements must be serialisable as a data stream, and deserialisable from a
/// data stream. The `read...`, `write...` and `from...` functions deal with
/// this.
/// 
/// ### Checksumming
/// 
/// A checksum of the serialised version of the element's data is required in
/// order (a) to validate data read from external sources (files) and (b) to
/// verify correct reconstruction of states of repository partitions.
/// 
/// This checksum can be calculated on the fly or could be cached.
/// 
/// ### Implementations
/// 
/// It is recommended that an implementation is written specific to each
/// use-case (using an enum if variadic data typing is needed). There is
/// however a default implementation for `String`.
/// 
/// A trivial example:
/// 
/// ```no_use
/// extern crate byteorder;
/// extern crate pippin;
/// 
/// use std::io::Write;
/// 
/// use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
/// use pippin::{Element, Result};
/// 
/// #[derive(PartialEq, Debug)]
/// struct Point { x: f64, y: f64 }
/// 
/// impl Element for Point {
///     fn write_buf(&self, writer: &mut Write) -> Result<()> {
///         try!(writer.write_f64::<LittleEndian>(self.x));
///         try!(writer.write_f64::<LittleEndian>(self.y));
///         Ok(())
///     }
///     fn read_buf(buf: &[u8]) -> Result<Self> {
///         let mut r: &mut &[u8] = &mut &buf[..];
///         Ok(Point {
///             x: try!(r.read_f64::<LittleEndian>()),
///             y: try!(r.read_f64::<LittleEndian>()),
///         })
///     }
/// }
/// ```
pub trait Element where Self: Sized+PartialEq+Eq+Debug+'static {
    // #0025: provide a choice of how to implement IO via a const bool?
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
    
    /// Create an instance from a buffer. This implementation wraps `read_buf`;
    /// write your own for more efficiency.
    fn from_vec(vec: Vec<u8>) -> Result<Self>{
        Self::read_buf(&vec)
    }
    
    /// Create an instance from a vector + a sum (equal to what `sum()` should
    /// output, so can be cached). The default implementation just calls
    /// `from_vec(vec)` and throws away the sum.
    fn from_vec_sum(vec: Vec<u8>, _sum: Sum) -> Result<Self>{
        Self::from_vec(vec)
    }
    
    /// This can either return a copy of an internally cached element sum or
    /// calculate one on the fly. It is used when inserting, removing or
    /// replacing an element in a state, and when merging states where the
    /// element differs.
    /// 
    /// The element sum is calculated via `Sum::elt_id(id, data)`.
    /// 
    /// Warning: this implementation panics if `write_buf` has an error!
    fn sum(&self, id: EltId) -> Sum {
        let mut buf = Vec::new();
        self.write_buf(&mut &mut buf).expect("write_buf does not fail in get_sum");
        Sum::elt_sum(id, &buf)
    }
}

impl Element for String {
    fn write_buf(&self, writer: &mut Write) -> Result<()> {
        writer.write_all(self.as_bytes())?;
        Ok(())
    }
    fn read_buf(buf: &[u8]) -> Result<Self> {
        let s = from_utf8(buf)?;
        Ok(s.to_string())
    }
    fn from_vec(vec: Vec<u8>) -> Result<Self>{
        Ok(String::from_utf8(vec)?)
    }
}
