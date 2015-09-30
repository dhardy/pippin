//! For calculating checksums

use std::io::{Read, Write, Result};

use crypto::digest::Digest;
use crypto::sha2::Sha256;

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
