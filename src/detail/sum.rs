//! For calculating checksums

use std::io::{Read, Result};

/*
pub struct Crc {
    crc: libc::c_ulong,
    amt: u32,
}*/

pub struct SumReader<R> {
    inner: R
}

// impl Crc {
//     pub fn new() -> Crc {
//         Crc { crc: 0, amt: 0 }
//     }
// 
//     pub fn sum(&self) -> libc::c_ulong { self.crc }
//     pub fn amt(&self) -> u32 { self.amt }
// 
//     pub fn update(&mut self, data: &[u8]) {
//         self.amt += data.len() as u32;
//         self.crc = unsafe {
//             ffi::mz_crc32(self.crc, data.as_ptr(), data.len() as libc::size_t)
//         };
//     }
// }

impl<R: Read> SumReader<R> {
    pub fn new(r: R) -> SumReader<R> {
        SumReader { inner: r }
    }
//     pub fn crc(&self) -> &Crc { &self.crc }
    pub fn into_inner(self) -> R { self.inner }
    pub fn inner(&mut self) -> &mut R { &mut self.inner }
}

impl<R: Read> Read for SumReader<R> {
    fn read(&mut self, into: &mut [u8]) -> Result<usize> {
        let amt = try!(self.inner.read(into));
//         self.crc.update(&into[..amt]);
        Ok(amt)
    }
}
