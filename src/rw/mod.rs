/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Pippin support for reading from and writing to files.
//! 
//! Many code patterns shamelessly lifted from Alex Crichton's flate2 library.

mod sum;
pub mod header;
pub mod snapshot;
pub mod commitlog;

use std::io::{Read, Write};
use std::iter::repeat;
use std::u32;

use byteorder::{ByteOrder, BigEndian, WriteBytesExt};

use commit::{CommitMeta, UserMeta, MetaFlags};
use error::{Result, ReadError};

// —————  module-private data and functions  —————

// Versions of header (all versions, including latest), encoded as an integer.
// All restrictions to specific versions should mention `HEAD_VERSIONS` in
// comments to aid searches.
// 
// Note: new versions can be implemented just by updating the three HEAD_...
// constants and updating code, so long as the code will still read old
// versions. The file format documentation should also be updated.
const HEAD_VERSIONS : [u32; 3] = [
    /* unsupported versions:
    2015_09_29, // initial standardisation
    2016_01_05, // add 'PARTID' to header blocks (snapshot only)
    2016_02_01, // add memory of new names of moved elements
    2016_02_21, // add metadata to commits (logs only)
    2016_02_22, // add metadata to snapshots (snapshots only)
    2016_02_27, // add parent state-sums to snapshots (snapshots only)
    */
    2016_03_10, // new element and state sums break compatibility
    2016_05_16, // support Bbbb header sections
    2016_08_15, // allow non-breaking extensions to commit-meta
];

/// Read metadata
/// 
/// This is a bit involved. It expects:
/// 
/// *   `r`: a reader
/// *   `buf`: a buffer of length at least 16 and with bytes 8..16 filled
/// *   `pos`: a counter, which needs incrementing by 16 after finishing 8 bytes from buf
fn read_meta(mut r: &mut Read, mut buf: &mut [u8], mut pos: &mut usize, format_ver: u32) -> Result<CommitMeta> {
    let secs = BigEndian::read_i64(&buf[8..16]);
    (*pos) += 16;
    
    r.read_exact(&mut buf[0..16])?;
    let (ext_len, ext_flags) = if format_ver < 2016_08_15 {
        if buf[0..4] != *b"CNUM" {
            return ReadError::err("unexpected contents (expected CNUM)", *pos, (0, 4));
        }
        (0, 0)
    } else {
        if buf[0] != b'F' {
            return ReadError::err("unexpected contents (expected F)", *pos, (0, 1));
        }
        let len = (buf[1] as usize) * 8;
        let flags = BigEndian::read_u16(&buf[2..4]);
        (len, flags)
    };
    let cnum = BigEndian::read_u32(&buf[4..8]);
    let mut ext_data: Vec<u8> = repeat(0).take(ext_len).collect();
    r.read_exact(&mut ext_data)?;
    
    if buf[8..10] != *b"XM" {
        return ReadError::err("unexpected contents (expected XM)", *pos, (8, 10));
    }
    let xm_type_txt = buf[10..12] == *b"TT";
    let xm_len = BigEndian::read_u32(&buf[12..16]) as usize;
    (*pos) += 16;
    
    let mut xm_data = vec![0; xm_len];
    r.read_exact(&mut xm_data)?;
    let xm = if xm_type_txt {
        UserMeta::Text(String::from_utf8(xm_data)
            .map_err(|_| ReadError::new("content not valid UTF-8", *pos, (0, xm_len)))?)
    } else {
        // even if xm_len > 0 we ignore it
        UserMeta::None
    };
    
    (*pos) += xm_len;
    let pad_len = 16 * ((xm_len + 15) / 16) - xm_len;
    if pad_len > 0 {
        r.read_exact(&mut buf[0..pad_len])?;
        (*pos) += pad_len;
    }
    
    let ext_flags = MetaFlags::from_raw(ext_flags);
    Ok(CommitMeta::new_explicit(cnum, secs, ext_flags, ext_data, xm)?)
}

/// Write commit metadata
fn write_meta(w: &mut Write, meta: &CommitMeta) -> Result<()> {
    w.write_i64::<BigEndian>(meta.timestamp())?;
    
    w.write_all(b"F")?;
    w.write_all(&[0u8; 1])?; // 0 extension data: we don't use this currently
    w.write_u16::<BigEndian>(meta.ext_flags().raw())?;
    w.write_u32::<BigEndian>(meta.number())?;
    // extension data would go here, but we don't currently have any
    
    match *meta.extra() {
        UserMeta::None => {
            // last four zeros is 0u32 encoded in bytes
            w.write_all(b"XM\x00\x00\x00\x00\x00\x00")?;
        },
        UserMeta::Text(ref txt) => {
            w.write_all(b"XMTT")?;
            assert!(txt.len() <= u32::MAX as usize);
            w.write_u32::<BigEndian>(txt.len() as u32)?;
            w.write_all(txt.as_bytes())?;
            let pad_len = 16 * ((txt.len() + 15) / 16) - txt.len();
            if pad_len > 0 {
                let padding = [0u8; 15];
                w.write_all(&padding[0..pad_len])?;
            }
        },
    }
    Ok(())
}
