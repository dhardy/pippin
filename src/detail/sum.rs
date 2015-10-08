//! For calculating checksums

use std::io::{Read, Write, Result};
use std::{ops};

use crypto::digest::Digest;
use crypto::sha2::Sha256;


// —————  sum storage / manipulation  —————

// NOTE: when simd is stable, it could be used
// use simd::u8x16;
/// A convenient way to manage and manipulate a checksum
pub struct Sum {
//     s1: u8x16, s2: u8x16
    s: [u8; 32]
}

impl Sum {
    /// A "sum" containing all zeros
    pub fn zero() -> Sum {
//         Sum { s1: u8x16::splat(0), s2: u8x16::splat(0) }
        Sum { s: [0u8; 32] }
    }
    
    /// True if sum equals that in a buffer
    pub fn eq(&self, arr: &[u8]) -> bool {
        assert_eq!(arr.len(), 32);
        for i in 0..32 {
            if self.s[i] != arr[i] { return false; }
        }
        return true;
    }
    
    /// Load from a u8 array
    pub fn load(arr: &[u8]) -> Sum {
        assert_eq!(arr.len(), 32);
//         Sum { s1: u8x16::load(&arr, 0), s2: u8x16::load(&arr, 16) }
        //TODO there must be a better way than this!
        let mut result = Sum::zero();
        for i in 0..32 {
            result.s[i] = arr[i];
        }
        result
    }
    
    /// Calculate from some data
    pub fn calculate(data: &[u8]) -> Sum {
        let mut hasher = Sha256::new();
        hasher.input(&data);
        let mut buf = [0u8; 32];
        assert_eq!(hasher.output_bytes(), buf.len());
        hasher.result(&mut buf);
        Sum::load(&buf)
    }
    
    /// Write the checksum bytes to a stream
    pub fn write(&self, w: &mut Write) -> Result<()> {
//         let mut buf = [0u8; 32];
//         s1.store(&mut buf, 0);
//         s2.store(&mut buf, 16);
        try!(w.write(&self.s));
        Ok(())
    }
}

impl ops::BitXor for Sum {
    type Output = Self;
    fn bitxor(self, rhs: Sum) -> Sum {
        //TODO optimise
        let mut result = Sum::zero();
        for i in 0..32 {
            result.s[i] = self.s[i] ^ rhs.s[i];
        }
        result
    }
}


// —————  hash calculators  —————

pub struct HashReader<H, R> {
    hasher: H,
    inner: R
}

impl<R: Read> HashReader<Sha256, R> {
    /// Create
    pub fn new256(r: R) -> HashReader<Sha256, R> {
        HashReader { hasher: Sha256::new(), inner: r }
    }
}

#[allow(dead_code)]
impl<H: Digest, R: Read> HashReader<H, R> {
    /// Get the hasher's Digest interface
    pub fn digest(&mut self) -> &mut Digest { &mut self.hasher }
    
    /// Get the inner reader
    pub fn inner(&mut self) -> &mut R { &mut self.inner }
    /// Consume self and return the inner reader
    pub fn into_inner(self) -> R { self.inner }
}

impl<H: Digest, R: Read> Read for HashReader<H, R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let len = try!(self.inner.read(buf));
        self.hasher.input(&buf[..len]);
        Ok(len)
    }
}


pub struct HashWriter<H, W> {
    hasher: H,
    inner: W
}

impl<W: Write> HashWriter<Sha256, W> {
    /// Create
    pub fn new256(w: W) -> HashWriter<Sha256, W> {
        HashWriter { hasher: Sha256::new(), inner: w }
    }
}

#[allow(dead_code)]
impl<H: Digest, W: Write> HashWriter<H, W> {
    /// Get the hasher's Digest interface
    pub fn digest(&mut self) -> &mut Digest { &mut self.hasher }
    
    /// Get the inner reader
    pub fn inner(&mut self) -> &mut W { &mut self.inner }
    /// Consume self and return the inner reader
    pub fn into_inner(self) -> W { self.inner }
}

impl<H: Digest, W: Write> Write for HashWriter<H, W> {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        let len = try!(self.inner.write(buf));
        if len > 0 {
            self.hasher.input(&buf[..len]);
        }
        Ok(len)
    }
    
    fn flush(&mut self) -> Result<()> {
        self.inner.flush()
    }
}
