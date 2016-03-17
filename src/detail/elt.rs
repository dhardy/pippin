/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Base type of elements stored in Pippin repositories

use std::fmt;
use std::fmt::Debug;
use std::io::{/*Read,*/ Write};
use std::str::from_utf8;
// use vec_map::VecMap;

use Sum;
use error::{Result};


/// A classification / partition number
/// 
/// This is a 40-bit number used to identify partitions and as part of an
/// element identifier. Classification also uses these numbers.
/// 
/// Supports `Into<u64>` to extract an encoded form. Can be reconstructed from
/// this via `try_from()`.
/// 
/// Supports `fmt::Display` (displays the same value as `id.into_num()`).
#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub struct PartId {
    // #0018: optimise usage as Option with NonZero?
    id: u64,
}
impl PartId {
    /// Convert from number, `n`, where `n > 0` and `n <= max_num()`. Panics if
    /// bounds are not met.
    // #0011: should this return an Option / Result?
    pub fn from_num(n: u64) -> PartId {
        assert!(n > 0 && n <= Self::max_num(), "PartId::from_num(n): n is invalid");
        PartId { id: n << 24 }
    }
    /// Convert to a number (same restrictions as for input to `from_num()`).
    pub fn into_num(self) -> u64 {
        self.id >> 24
    }
    /// Reconstructs from a value returned by `into()` (see `Into<u64>` impl).
    pub fn try_from(id: u64) -> Option<PartId> {
        if id == 0 || (id & 0xFF_FFFF) != 0 { return None; }
        Some(PartId { id: id })
    }
    /// Create from a partition identifier plus a number. The number `n` must
    /// be no more than `EltId::max()`.
    pub fn elt_id(self, n: u32) -> EltId {
        assert!(n <= EltId::max(), "PartId::elt_id(n): n is invalid");
        EltId { id: self.id + n as u64 }
    }
    /// The maximum number which can be passed to `from_num()`
    pub fn max_num() -> u64 {
       0xFF_FFFF_FFFF
    }
}
impl Into<u64> for PartId {
    fn into(self) -> u64 {  
        self.id
    }
}
impl fmt::Display for PartId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.into_num())
    }
}

/// An element identifier
/// 
/// This encodes both a partition identifier (`PartId`) and an element number
/// (unique within the partition).
/// 
/// Supports `From` (`EltId::from(n)`) to convert from a `u64` (this panics if
/// the value is not a valid identifier). Supports `Into` (`pn.into()`) to
/// convert to a `u64`.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash)]
pub struct EltId {
    // #0018: optimise usage as Option with NonZero?
    id: u64,
}
impl EltId {
    /// Extract the partition identifier
    pub fn part_id(self) -> PartId {
        PartId::try_from(self.id & 0xFFFF_FFFF_FF00_0000).unwrap()
    }
    /// Extract the element number (this is a 24-bit number)
    pub fn elt_num(self) -> u32 {
        (self.id & 0xFF_FFFF) as u32
    }
    /// Get the next element identifier, wrapping to zero if necessary, but
    /// keeping the same partition identifier.
    pub fn next_elt(self) -> EltId {
        let mut num = self.elt_num() + 1;
        if num > Self::max() { num = 0; }
        self.part_id().elt_id(num)
    }
    /// Maximum value which `elt_num()` can return and can be passed to
    /// `PartId::elt_id()`.
    pub fn max() -> u32 {
        0xFF_FFFF
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
/// use pippin::{ElementT, Result};
/// 
/// #[derive(PartialEq, Debug)]
/// struct Point { x: f64, y: f64 }
/// 
/// impl ElementT for Point {
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
pub trait ElementT where Self: Sized+PartialEq+Debug {
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
    
    /// This can either return a copy of an internally stored element sum or
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

impl ElementT for String {
    fn write_buf(&self, writer: &mut Write) -> Result<()> {
        try!(writer.write(self.as_bytes()));
        Ok(())
    }
    fn read_buf(buf: &[u8]) -> Result<Self> {
        let s = try!(from_utf8(buf));
        Ok(s.to_string())
    }
    fn from_vec(vec: Vec<u8>) -> Result<Self>{
        Ok(try!(String::from_utf8(vec)))
    }
}
