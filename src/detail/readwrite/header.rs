//! Read and write support for Pippin file headers.

use std::{io};
use std::cmp::min;
use std::result::Result as stdResult;

use ::error::{Result, ArgError, ReadError, make_io_err};
use ::util::rtrim;
use super::{sum, fill};

const HEAD_SNAPSHOT : [[u8; 16]; 2] = [*b"PIPPINSS20150929",
    *b"PIPPINSS20160105"];
const HEAD_COMMITLOG : [u8; 16] = *b"PIPPINCL20150929";
const SUM_SHA256 : [u8; 16] = *b"HSUM SHA-2 256\x00\x00";

pub enum FileType {
    Snapshot,
    CommitLog,
}

// Information stored in a file header
pub struct FileHeader {
    pub ftype: FileType,
    /// Repo name
    pub name: String,
    pub remarks: Vec<String>,
    pub user_fields: Vec<Vec<u8>>
}

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
pub fn read_head(r: &mut io::Read) -> Result<FileHeader> {
    // A reader which also calculates a checksum:
    let mut sum_reader = sum::HashReader::new256(r);
    
    let mut pos: usize = 0;
    let mut buf = vec![0; 16];
    
    try!(fill(&mut sum_reader, &mut buf[0..16], pos));
    let ftype = if buf == HEAD_SNAPSHOT[0] || buf == HEAD_SNAPSHOT[1] {
        FileType::Snapshot
    } else if buf == HEAD_COMMITLOG {
        FileType::CommitLog
    } else {
        return ReadError::err("not a known Pippin file format", pos, (0, 16));
    };
    pos += 16;
    
    try!(fill(&mut sum_reader, &mut buf[0..16], pos));
    let repo_name = match String::from_utf8(rtrim(&buf, 0).to_vec()) {
        Ok(name) => name,
        Err(_) => return ReadError::err("repo name not valid UTF-8", pos, (0, 16))
    };
    pos += 16;
    
    let mut header = FileHeader{
        ftype: ftype,
        name: repo_name,
        remarks: Vec::new(),
        user_fields: Vec::new(),
    };
    
    loop {
        try!(fill(&mut sum_reader, &mut buf[0..16], pos));
        let (block, off): (&[u8], usize) = if buf[0] == b'H' {
            pos += 1;
            (rtrim(&buf[1..16], 0), 1)
        } else if buf[0] == b'Q' {
            let x: usize = match buf[1] {
                b'1' ... b'9' => buf[1] - b'0',
                b'A' ... b'Z' => buf[1] + 10 - b'A',
                _ => return ReadError::err("header section Qx... has invalid length specification 'x'", pos, (0, 2))
            } as usize;
            let len = x * 16;
            if buf.len() < len { buf.resize(len, 0); }
            try!(fill(&mut sum_reader, &mut buf[16..len], pos));
            pos += 2;
            (rtrim(&buf[2..len], 0), 2)
        } else {
            return ReadError::err("unexpected header contents", pos, (0, 1));
        };
        
        if block[0..3] == *b"SUM" {
            if block[3..] == SUM_SHA256[4..14] {
                /* we don't support any other checksum else yet, so don't need
                 * to configure anything here */
            }else {
                return ReadError::err("unknown checksum format", pos, (3+off, 13+off))
            };
            break;      // "HSUM" must be last item of header before final checksum
        } else if block[0] == b'R' {
            header.remarks.push(try!(String::from_utf8(rtrim(&block, 0).to_vec())));
        } else if block[0] == b'U' {
            header.user_fields.push(rtrim(&block[1..], 0).to_vec());
        } else if block[0] == b'O' {
            // Match optional extensions here; we currently have none
        } else if block[0] >= b'A' && block[0] <= b'Z' {
            // Match important extensions here; we currently have none
            // No match:
            // #0017: proper output of warnings
            println!("Warning: unrecognised file extension:");
            println!("{:?}", block);
        } else {
            // Match any other block rules here.
        }
        pos += block.len();
    }
    
    // Read checksum (assume SHA-256)
    let mut buf32 = [0u8; 32];
    try!(fill(&mut sum_reader.inner(), &mut buf32, pos));
    assert_eq!( sum_reader.digest().output_bytes(), 32 );
    let mut sum32 = [0u8; 32];
    sum_reader.digest().result(&mut sum32);
    if buf32 != sum32 {
        return ReadError::err("header checksum invalid", pos, (0, 32));
    }
    
    Ok(header)
}

/// Write a file header.
pub fn write_head(header: &FileHeader, w: &mut io::Write) -> Result<()> {
    use std::io::Write;
    
    // A writer which calculates the checksum of what was written:
    let mut sum_writer = sum::HashWriter::new256(w);
    
    match header.ftype {
        FileType::Snapshot => {
            try!(sum_writer.write(&HEAD_SNAPSHOT[1]));
        },
        FileType::CommitLog => {
            try!(sum_writer.write(&HEAD_COMMITLOG));
        },
    };
    try!(validate_repo_name(&header.name));
    let len = try!(sum_writer.write(header.name.as_bytes()));
    try!(pad(&mut sum_writer, 16 - len));
    
    for rem in &header.remarks {
        let b = rem.as_bytes();
        if b[0] != b'R' {
            return ArgError::err("remark does not start 'R'");
        }
        if b.len() <= 15 {
            try!(sum_writer.write(b"H"));
            try!(sum_writer.write(b));
            try!(pad(&mut sum_writer, 15 - b.len()));
        } else if b.len() <= 16 * 36 - 2 {
            let n = (b.len() + 2 /* Qx */ + 15 /* round up */) / 16;
            let l = [b'Q', if n <= 9 { b'0' + n as u8 } else { b'A' - 10 + n as u8 } ];
            try!(sum_writer.write(&l));
            try!(sum_writer.write(b));
            try!(pad(&mut sum_writer, n * 16 - b.len() + 2));
        } else {
            return ArgError::err("remark too long");
        }
    }
    
    for uf in &header.user_fields {
        let mut l = [b'Q', b'H', b'U'];
        if uf.len() <= 14 {
            try!(sum_writer.write(&l[1..3]));
            try!(sum_writer.write(&uf));
            try!(pad(&mut sum_writer, 14 - uf.len()));
        } else if uf.len() <= 16 * 36 - 3 {
            let n = (uf.len() + 3 /* QxU */ + 15 /* round up */) / 16;
            l[1] = if n <= 9 { b'0' + n as u8 } else { b'A' - 10 + n as u8 };
            try!(sum_writer.write(&l[0..3]));
            try!(sum_writer.write(&uf));
            try!(pad(&mut sum_writer, n * 16 - uf.len() - 3));
        } else {
            return ArgError::err("user field too long");
        }
    }
    
    try!(sum_writer.write(&SUM_SHA256));
    
    // Write the checksum of everything above:
    assert_eq!( sum_writer.digest().output_bytes(), 32 );
    let mut sum32 = [0u8; 32];
    sum_writer.digest().result(&mut sum32);
    let w2 = sum_writer.into_inner();
    try!(w2.write(&sum32));
    
    fn pad<W: Write>(w: &mut W, n1: usize) -> Result<()> {
        let zeros = [0u8; 16];
        let mut n = n1;
        while n > 0 {
            n -= match try!(w.write(&zeros[0..min(n, zeros.len())])) {
                0 => return make_io_err(io::ErrorKind::WriteZero, "write failed"),
                x => x
            };
        }
        Ok(())
    }
    
    Ok(())
}

#[test]
fn read_header() {
    // Note: checksum calculated with Python 3:
    // import hashlib
    // hashlib.sha256(b"PIPPINSS20150929...").digest()
    let head = b"PIPPINSS20150929\
                test AbC \xce\xb1\xce\xb2\xce\xb3\x00\
                HRemark 12345678\
                HOoptional rule\x00\
                HUuser rule\x00\x00\x00\x00\x00\
                Q2REM  completel\
                y pointless text\
                H123456789ABCDEF\
                HSUM SHA-2 256\x00\x00\
                \x16\x0c\xafcWm\xe3i\xb8\xf6T\x92\x05\xb7\xd98\
                \x92\x86\xb8\xb6\x15>\x00\x86\"\xfd\xff\x97\xfcAp\xa1";
    let header = read_head(&mut &head[..]).unwrap();
    assert_eq!(header.name, "test AbC αβγ");
    assert_eq!(header.remarks, vec!["Remark 12345678", "REM  completely pointless text"]);
    assert_eq!(header.user_fields, vec![b"user rule"]);
}

#[test]
fn write_header() {
    let header = FileHeader {
        ftype: FileType::Snapshot,
        name: "Ähnliche Unsinn".to_string(),
        remarks: vec!["Remark ω".to_string(), "R Quatsch Quatsch Quatsch".to_string()],
        user_fields: vec![b" rsei noasr auyv 10()% xovn".to_vec()]
    };
    let mut buf = Vec::new();
    write_head(&header, &mut buf).unwrap();
    let expected = b"PIPPINSS20160105\
            \xc3\x84hnliche Unsinn\
            HRemark \xcf\x89\x00\x00\x00\x00\x00\x00\
            Q2R Quatsch Quatsch \
            Quatsch\x00\x00\x00\x00\x00\x00\x00\x00\x00\
            Q2U rsei noasr a\
            uyv 10()% xovn\x00\x00\
            HSUM SHA-2 256\x00\x00\
            (q9\xff\xbb/\x8d\xd0\xfb\x9dDxys\xedw\
            \x9c8\xfd\xba\x9f40\xdaK\xad\xcbm\xdf\x9cs\xbc";
    assert_eq!(&buf[..], &expected[..]);
}
