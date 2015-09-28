//! For calculating checksums

use std::io::{Read, Result};

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

impl<H: Digest, R: Read> HashReader<H, R> {
    /// Get the size of the output hash in bytes
    pub fn hash_bytes(&self) -> usize { self.hasher.output_bytes() }
    /// Format the generated hash to a buffer of size at least hash_bytes()
    pub fn result(&mut self, buf: &mut [u8]) { self.hasher.result(buf); }
    /// Format the generated hash to a new String
    pub fn hash_str(&mut self) -> String { self.hasher.result_str() }
    /// Get the hasher's Digest interface
    pub fn digest(&mut self) -> &mut H { &mut self.hasher }
    
    /// Get the inner reader
    pub fn inner(&mut self) -> &mut R { &mut self.inner }
    /// Consume self and return the inner reader
    pub fn into_inner(self) -> R { self.inner }
}

impl<H: Digest, R: Read> Read for HashReader<H, R> {
    fn read(&mut self, into: &mut [u8]) -> Result<usize> {
        let len = try!(self.inner.read(into));
        self.hasher.input(&into[..len]);
        Ok(len)
    }
}
