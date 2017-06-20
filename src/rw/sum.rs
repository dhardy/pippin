/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! For calculating checksums

use std::io::{Read, Write, Result};

use crypto::digest::Digest;
// use crypto::sha2::Sha256;
use crypto::blake2b::Blake2b;
use byteorder::{ByteOrder, BigEndian};

use elt::{EltId, PartId};
use commit::{CommitMeta, UserMeta};
use sum::{Sum, SUM_BYTES};


// Internal type / constructor for easy configuration.
// type Hasher = Sha256;
type Hasher = Blake2b;
fn mk_hasher() -> Hasher {
//     Hasher::new()
    Hasher::new(SUM_BYTES)
}

impl Sum {
    /// Calculate an element sum
    pub fn elt_sum(elt_id: EltId, data: &[u8]) -> Sum {
        let mut hasher = mk_hasher();
        let mut buf = [0u8; 8];
        BigEndian::write_u64(&mut buf, elt_id.into());
        hasher.input(&buf);
        hasher.input(data);
        Sum::load_hasher(hasher)
    }
    /// Calculate a partition's meta-data sum
    pub fn state_meta_sum(part_id: PartId, parents: &[Sum], meta: &CommitMeta) -> Sum {
        let mut hasher = mk_hasher();
        let mut buf = Vec::from("PpppPpppCNUMNnnnTtttTttt");
        if SUM_BYTES > buf.len() {
            // for second use of buf below
            buf.resize(SUM_BYTES, 0);
        }
        BigEndian::write_u64(&mut buf[0..8], part_id.into());
        assert_eq!(buf[8..12], *b"CNUM");
        BigEndian::write_u32(&mut buf[12..16], meta.number());
        BigEndian::write_i64(&mut buf[16..24], meta.timestamp());
        
        hasher.input(&buf[0..24]);
        for parent in parents {
            parent.write_to(&mut &mut buf[..]).expect("writing to buf");
            hasher.input(&buf);
        }
        match *meta.extra() {
            UserMeta::None => {},
            UserMeta::Text(ref text) => {
                hasher.input(text.as_bytes());
            },
        }
        Sum::load_hasher(hasher)
    }
    /// Calculate a standard checksum
    pub fn calculate(data: &[u8]) -> Sum {
        let mut hasher = mk_hasher();
        hasher.input(data);
        Sum::load_hasher(hasher)
    }
    /// Load from a hasher
    fn load_hasher(mut hasher: Hasher) -> Sum {
        let mut buf = [0u8; SUM_BYTES];
        assert_eq!(hasher.output_bytes(), buf.len());
        hasher.result(&mut buf);
        Sum::load(&buf)
    }
}


// —————  hash calculators  —————

pub struct HashReader<R> {
    hasher: Hasher,
    inner: R
}

impl<R: Read> HashReader<R> {
    /// Create
    pub fn new(r: R) -> HashReader<R> {
        HashReader { hasher: mk_hasher(), inner: r }
    }
}

#[allow(dead_code)]
impl<R: Read> HashReader<R> {
    /// Get the hasher's Digest interface
    pub fn digest(&mut self) -> &mut Digest { &mut self.hasher }
    /// Make a Sum from the digest
    pub fn sum(&mut self) -> Sum {
        let mut buf = [0u8; SUM_BYTES];
        assert_eq!(self.hasher.output_bytes(), buf.len());
        self.hasher.result(&mut buf);
        Sum::load(&buf)
    }
    
    /// Get the inner reader
    pub fn inner(&mut self) -> &mut R { &mut self.inner }
    /// Consume self and return the inner reader
    pub fn into_inner(self) -> R { self.inner }
}

impl<R: Read> Read for HashReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let len = self.inner.read(buf)?;
        self.hasher.input(&buf[..len]);
        Ok(len)
    }
}


pub struct HashWriter<W> {
    hasher: Hasher,
    inner: W
}

impl<W: Write> HashWriter<W> {
    /// Create
    pub fn new(w: W) -> HashWriter<W> {
        HashWriter { hasher: mk_hasher(), inner: w }
    }
}

#[allow(dead_code)]
impl<W: Write> HashWriter<W> {
    /// Get the hasher's Digest interface
    pub fn digest(&mut self) -> &mut Digest { &mut self.hasher }
    /// Make a Sum from the digest
    pub fn sum(&mut self) -> Sum {
        let mut buf = [0u8; SUM_BYTES];
        assert_eq!(self.hasher.output_bytes(), buf.len());
        self.hasher.result(&mut buf);
        Sum::load(&buf)
    }
    
    /// Get the inner writer
    pub fn inner(&mut self) -> &mut W { &mut self.inner }
    /// Consume self and return the inner writer
    pub fn into_inner(self) -> W { self.inner }
}

impl<W: Write> Write for HashWriter<W> {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        let len = self.inner.write(buf)?;
        if len > 0 {
            self.hasher.input(&buf[..len]);
        }
        Ok(len)
    }
    
    fn flush(&mut self) -> Result<()> {
        self.inner.flush()
    }
}
