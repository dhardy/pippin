/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Read and write support for Pippin file headers.

use std::io::{Read, Write, ErrorKind};
use std::cmp::min;
use std::result::Result as stdResult;

use byteorder::{ByteOrder, BigEndian, WriteBytesExt};

use elt::PartId;
use readwrite::sum;
use error::{Result, ArgError, ReadError, make_io_err};
use sum::SUM_BYTES;
use util::rtrim;

// Snapshot header. This is the latest version.
const HEAD_SNAPSHOT : [u8; 16] = *b"PIPPINSS20160815";
// Commit log header. This is the latest version.
const HEAD_COMMITLOG : [u8; 16] = *b"PIPPINCL20160815";
// Versions of header (all versions, including latest), encoded as an integer.
// All restrictions to specific versions should mention `HEAD_VERSIONS` in
// comments to aid searches.
// 
// Note: new versions can be implemented just by updating the three HEAD_...
// constants and updating code, so long as the code will still read old
// versions. The file format documentation should also be updated.
pub const HEAD_VERSIONS : [u32; 3] = [
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
const SUM_SHA256 : [u8; 16] = *b"HSUM SHA-2 256\x00\x00";
const SUM_BLAKE2_16 : [u8; 16] = *b"HSUM BLAKE2 16\x00\x00";
const PARTID : [u8; 8] = *b"HPARTID ";

/// File type and version.
/// 
/// Version is encoded as an integer; see `HEAD_VERSIONS` constant.
/// 
/// The version is set when a header is read but ignored when the header is
/// written. When creating an instance you can normally just use version 0.
pub enum FileType {
    /// File is a snapshot
    Snapshot(u32),
    /// File is a commit log
    CommitLog(u32),
}
impl FileType {
    /// Extract the version number regardless of file type (should be one of
    /// the HEAD_VERSIONS numbers or zero).
    pub fn ver(&self) -> u32 {
        match self {
            &FileType::Snapshot(v) => v,
            &FileType::CommitLog(v) => v,
        }
    }
}

/// Types of user-data which can be stored in header fields.
/// 
/// Maximum length of each field is currently 2^24 - 5  bytes (almost 16 MB of
/// text / data).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum UserData {
    /// Free-form data
    Data(Vec<u8>),
    /// UTF-8, stored as a "remark"
    Text(String),
}

/// Information stored in a file header
pub struct FileHeader {
    /// File type: snapshot or log file.
    pub ftype: FileType,
    /// Repo name. Always present.
    pub name: String,
    /// Partition identifier.
    pub part_id: PartId,
    /// User data fields, remarks, etc.
    pub user: Vec<UserData>,
}

// Decodes from a string to the format used in HEAD_VERSIONS. Returns zero on
// error.
fn read_head_version(s: &[u8]) -> u32 {
    let mut v = 0;
    for c in s {
        if *c < b'0' || *c > b'9' { return 0; }
        v = 10 * v + (*c - b'0') as u32;
    }
    v
}

/// Performs basic validation of a repository name. This same function is used
/// on the name given to a new partition or repository on creation.
pub fn validate_repo_name(name: &str) -> stdResult<(), ArgError> {
    if name.len() == 0 {
        return Err(ArgError::new("repo name missing (length 0)"));
    }
    if name.as_bytes().len() > 16 {
        return Err(ArgError::new("repo name too long"));
    }
    Ok(())
}

/// Read a file header.
pub fn read_head(reader: &mut Read) -> Result<FileHeader> {
    // A reader which also calculates a checksum:
    let mut r = sum::HashReader::new(reader);
    
    let mut pos: usize = 0;
    let mut buf = vec![0; 32];
    
    r.read_exact(&mut buf[0..16])?;
    let head_version = read_head_version(&buf[8..16]);
    if !HEAD_VERSIONS.contains(&head_version) {
        return ReadError::err("Pippin file of incompatible version", pos, (0, 16));
    }
    let ftype = if buf[0..8] == HEAD_SNAPSHOT[0..8] {
        FileType::Snapshot(head_version)
    } else if buf[0..8] == HEAD_COMMITLOG[0..8] {
        FileType::CommitLog(head_version)
    } else {
        return ReadError::err("not a known Pippin file format", pos, (0, 16));
    };
    pos += 16;
    
    r.read_exact(&mut buf[0..16])?;
    let repo_name = match String::from_utf8(rtrim(&buf, 0).to_vec()) {
        Ok(name) => name,
        Err(_) => return ReadError::err("repo name not valid UTF-8", pos, (0, 16))
    };
    pos += 16;
    
    let mut part_id = None;
    let mut user_fields = Vec::new();
    loop {
        r.read_exact(&mut buf[0..16])?;
        let (block, off): (&[u8], usize) = if buf[0] == b'H' {
            pos += 1;
            (&buf[1..16], 1)
        } else if buf[0] == b'Q' {
            let x: usize = match buf[1] {
                b'1' ... b'9' => buf[1] - b'0',
                b'A' ... b'Z' => buf[1] + 10 - b'A',
                _ => return ReadError::err("header section Qx... has invalid length specification 'x'", pos, (0, 2))
            } as usize;
            let len = x * 16;
            if buf.len() < len { buf.resize(len, 0); }
            r.read_exact(&mut buf[16..len])?;
            pos += 2;
            (&buf[2..len], 2)
        } else if buf[0] == b'B' {
            let len: usize = ((buf[1] as usize) << 16)
                           + ((buf[2] as usize) << 8)
                           +  (buf[3] as usize);
            let padded = ((len + 15) / 16) * 16; // round up
            if buf.len() < padded { buf.resize(padded, 0); }
            r.read_exact(&mut buf[16..padded])?;
            pos += 4;
            (&buf[4..len], 4)
        } else {
            return ReadError::err("unexpected header contents", pos, (0, 1));
        };
        
        if block[0..3] == *b"SUM" {
            if rtrim(&block[3..], 0) == &SUM_BLAKE2_16[4..14] {
                /* we don't support any other checksum at run-time, so don't need
                 * to configure anything here */
            } else if rtrim(&block[3..], 0) == &SUM_SHA256[4..14] {
                return ReadError::err("file uses SHA256 checksum; program not configured for this",
                    pos, (3+off, 13+off))
            }else {
                return ReadError::err("unknown checksum format", pos, (3+off, 13+off))
            };
            break;      // "HSUM" must be last item of header before final checksum
        } else if block[0..7] == PARTID[1..] {
            if part_id != None {
                return ReadError::err("repeat of PARTID", pos, (off, off+7));
            }
            let id = BigEndian::read_u64(&block[7..15]);
            part_id = Some(PartId::try_from(id)?);
        } else if block[0] == b'R' {
            user_fields.push(UserData::Text(String::from_utf8(rtrim(&block[1..], 0).to_vec())?));
        } else if block[0] == b'U' {
            user_fields.push(UserData::Data(block[1..].to_vec()));
        } else if block[0] >= b'A' && block[0] <= b'Z' {
            // Match unknown essential extensions here
            // Note: we *could* go ahead and read file with caution, but how
            // should we proceed when we know we missed something important?
            error!("Unknown essential header block: {}", String::from_utf8_lossy(block));
            return ReadError::err("unknown essential header block", pos, (off, off+block.len()));
        } else if block[0] >= b'a' && block[0] <= b'z' {
            // Match unknown inessential extensions here
            trace!("Ignoring unknown inessential header block: {}", String::from_utf8_lossy(block));
        } else {
            // Match any other block rules here.
            error!("Invalid header block: {}", String::from_utf8_lossy(block));
            return ReadError::err("invalid header block", pos, (off, off+block.len()));
        }
        pos += block.len();
    }
    
    let part_id = part_id.ok_or(ReadError::new("no PARTID specified", pos, (0, 0)))?;
    
    // Read checksum:
    let sum = r.sum();
    let mut r = r.into_inner();
    r.read_exact(&mut buf[0..SUM_BYTES])?;
    if !sum.eq(&buf[0..SUM_BYTES]) {
        return ReadError::err("header checksum invalid", pos, (0, SUM_BYTES));
    }
    
    Ok(FileHeader{
        ftype: ftype,
        name: repo_name,
        part_id: part_id,
        user: user_fields,
    })
}

/// Write a file header.
pub fn write_head(header: &FileHeader, writer: &mut Write) -> Result<()> {
    // A writer which calculates the checksum of what was written:
    let mut w = sum::HashWriter::new(writer);
    
    match header.ftype {
        // Note: we always write in the latest version, even if we read from an old one
        FileType::Snapshot(_) => {
            w.write(&HEAD_SNAPSHOT)?;
        },
        FileType::CommitLog(_) => {
            w.write(&HEAD_COMMITLOG)?;
        },
    };
    validate_repo_name(&header.name)?;
    let len = w.write(header.name.as_bytes())?;
    pad(&mut w, 16 - len)?;
    
    w.write(&PARTID)?;
    w.write_u64::<BigEndian>(header.part_id.into())?;
    
    for u in &header.user {
        // We allow padding in text mode:
        let (t, uf, is_text) = match u {
            &UserData::Data(ref b) => (b'U', &b[..], false),
            &UserData::Text(ref t) => (b'R', t.as_bytes(), true),
        };
        let mut l = [b'B', 0, b'Q', b'H', t];
        if uf.len() <= 14 && (is_text || uf.len() == 14) {
            w.write(&l[3..5])?;
            w.write(&uf)?;
            pad(&mut w, 14 - uf.len())?;
        } else if uf.len() + 3 <= 16 * 36 && 
            (is_text || (uf.len() + 3) % 16 == 0)
        {
            let n = (uf.len() + 3 /* QxU */ + 15 /* round up */) / 16;
            l[3] = if n <= 9 { b'0' + n as u8 } else { b'A' - 10 + n as u8 };
            w.write(&l[2..5])?;
            w.write(&uf)?;
            pad(&mut w, n * 16 - uf.len() - 3)?;
        } else if uf.len() <= (2 << 24) - 5 {
            let len = uf.len() + 5; // length written includes leading `Bbbb` and 'U' or 'R'
            l[1] = ((len >> 16) & 0xFF) as u8;
            l[2] = ((len >> 8) & 0xFF) as u8;
            l[3] = (len & 0xFF) as u8;
            w.write(&l[0..5])?;
            w.write(&uf)?;
            pad(&mut w, ((len + 15) / 16) * 16 - len)?;
        } else {
            return ArgError::err("user field too long");
        }
    }
    
    w.write(&SUM_BLAKE2_16)?;
    
    // Write the checksum of everything above:
    let sum = w.sum();
    sum.write(&mut w.into_inner())?;
    
    fn pad<W: Write>(w: &mut W, n1: usize) -> Result<()> {
        let zeros = [0u8; 16];
        let mut n = n1;
        while n > 0 {
            n -= match w.write(&zeros[0..min(n, zeros.len())])? {
                0 => return make_io_err(ErrorKind::WriteZero, "write failed"),
                x => x
            };
        }
        Ok(())
    }
    
    Ok(())
}

#[test]
fn read_header() {
    let head = b"PIPPINSS20160516\
                test AbC \xce\xb1\xce\xb2\xce\xb3\x00\
                HPARTID \x00\x00\x00\x01\x01\x00\x00\x00\
                HRemark 12345678\
                Hoptional rule\x00\x00\
                B\x00\x00\x0eUuser rule\x00\x00\
                HUuser rule\x00\x00\x00\x00\x00\
                Q2REM  completel\
                y pointless text\
                Hi123456789ABCDE\
                HSUM BLAKE2 16\x00\x00\
                }f\xcb!\xbe\xa0\x9b\xdf\xa9\x03\x8c\x84+a\xe2\x8eMG!\xe0\xf6,^t0!\xeb\xc04\xff\\\xe5";
    
    use sum::Sum;
    let sum = Sum::calculate(&head[0..head.len() - SUM_BYTES]);
    println!("Checksum: '{}'", sum.byte_string());
    let header = match read_head(&mut &head[..]) {
        Ok(h) => h,
        Err(e) => { panic!("{}", e); }
    };
    assert_eq!(header.name, "test AbC αβγ");
    assert_eq!(header.part_id, PartId::from_num(257));
    assert_eq!(header.user.len(), 4);
    assert_eq!(header.user[0], UserData::Text("emark 12345678".to_string()));
    assert_eq!(header.user[1], UserData::Data(b"user rule".to_vec()));
    assert_eq!(header.user[2], UserData::Data(b"user rule\x00\x00\x00\x00\x00".to_vec()));
    assert_eq!(header.user[3], UserData::Text("EM  completely pointless text".to_string()));
}

#[test]
fn write_header() {
    let header = FileHeader {
        ftype: FileType::Snapshot(0 /*version should be ignored*/),
        name: "Ähnliche Unsinn".to_string(),
        part_id: PartId::from_num(123),
        user: vec![
            UserData::Text("Remark ω".to_string()),
            UserData::Text(" Quatsch Quatsch Quatsch".to_string()),
            UserData::Data(b"0123456789abcdefghijklmnopqrs".to_vec()),
            UserData::Data(b" rsei noasr auyv 10()% xovn".to_vec()),
        ],
    };
    let mut buf = Vec::new();
    write_head(&header, &mut buf).unwrap();
    
    let expected = b"PIPPINSS20160815\
            \xc3\x84hnliche Unsinn\
            HPARTID \x00\x00\x00\x00\x7B\x00\x00\x00\
            HRRemark \xcf\x89\x00\x00\x00\x00\x00\
            Q2R Quatsch Quatsch \
            Quatsch\x00\x00\x00\x00\x00\
            Q2U0123456789abc\
            defghijklmnopqrs\
            B\x00\x00\x20U rsei noasr a\
            uyv 10()% xovn\
            HSUM BLAKE2 16\x00\x00\
            \xbe\x89\\\x86\x82\x91\xbbn\xdc\xfb\x99X\x17i,\"\xf4\xce,\xcd\xc5\xbf\xc3\x8b\x13\xbcI\x1b\xd3dI\xed";
    use ::util::ByteFormatter;
    println!("Checksum: '{}'", ByteFormatter::from(&buf[buf.len()-SUM_BYTES..buf.len()]));
    if buf[..] != expected[..] {
        println!("generated: {}", ByteFormatter::from(&buf));
        println!("expected : {}", ByteFormatter::from(expected));
        assert!(false);
    }
}
