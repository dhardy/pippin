//! Base type of elements stored in Pippin repositories

use std::fmt::Debug;
use std::io::{/*Read,*/ Write};
use std::str::from_utf8;
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
    
    /// Create an instance from a buffer. This implementation wraps `read_buf`;
    /// write your own for more efficiency.
    fn from_vec(vec: Vec<u8>) -> Result<Self>{
        Self::read_buf(&vec)
    }
    
    /// This can either return a copy of an internally stored sum or calculate
    /// one on the fly. It is used when inserting, removing or replacing an
    /// element in a state, and when merging states where the element differs.
    /// 
    /// Warning: this implementation panics if `write_buf` has an error!
    fn sum(&self) -> Sum {
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
    fn from_vec(vec: Vec<u8>) -> Result<Self>{
        Ok(try!(String::from_utf8(vec)))
    }
}
