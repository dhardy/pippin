//! Pippin implementation details.

//! Many code forms shamelessly lifted from Alex Crichton's flate2 library.

mod sum;

pub use self::read::read_head;

// Information stored in a file header
pub struct FileHeader {
    /// Repo name
    pub name: String,
    pub remarks: Vec<String>,
    pub user_fields: Vec<Vec<u8>>
}

mod read {
    use std::{io, mem, iter, cmp};
    use ::detail::{FileHeader, sum};
    use ::error::{Result, Error};
    
    /// Read a file header.
    /// 
    /// Note that if the repo name is not valid UTF-8, conversion is lossy.
    pub fn read_head(r: &mut io::Read) -> Result<FileHeader> {
        // A reader which also calculates a checksum:
        let mut sum_reader = sum::HashReader::new256(r);
        
        let mut pos: usize = 0;
        let mut buf = Vec::new();
        buf.extend(iter::repeat(0).take(16));   // resize to 16 bytes
        
        try!(fill(&mut sum_reader, &mut buf[0..16], pos));
        if buf != *b"PIPPINSS20150929" {
            return Err(Error::read("not a known Pippin file format", pos));
        }
        pos += 16;
        
        try!(fill(&mut sum_reader, &mut buf[0..16], pos));
        let repo_name = match String::from_utf8(rtrim(&buf, 0).to_vec()) {
            Ok(name) => name,
            Err(_) => return Err(Error::read("repo name not valid UTF-8", pos))
        };
        pos += 16;
        
        let mut remarks = Vec::new();
        let mut user_fields = Vec::new();
        
        loop {
            try!(fill(&mut sum_reader, &mut buf[0..16], pos));
            let block = if buf[0] == b'H' {
                pos += 1;
                &buf[1..16]
            } else if buf[0] == b'Q' {
                let x: usize = match buf[1] {
                    b'1' ... b'9' => buf[1] - b'0',
                    b'A' ... b'Z' => buf[1] + 10 - b'A',
                    _ => return Err(Error::read("header section Qx... has invalid length specification 'x'", pos))
                } as usize;
                let len = x * 16;
                if buf.len() < len {
                    let by = len - buf.len();
                    buf.extend(iter::repeat(0).take(by));
                }
                try!(fill(&mut sum_reader, &mut buf[16..len], pos));
                pos += 2;
                &buf[2..len]
            } else {
                return Err(Error::read("unexpected header contents", pos));
            };
            
            if block[0..3] == *b"SUM" {
                match &block[3..] {
                    b" SHA-2 256\x00\x00" => { /* we don't support anything else */ },
                    _ => return Err(Error::read("unknown checksum format", pos))
                };
                break;      // "HSUM" must be last item of header before final checksum
            } else if block[0] == b'R' {
                remarks.push(try!(String::from_utf8(rtrim(&block, 0).to_vec())));
            } else if block[0] == b'U' {
                user_fields.push(block.to_vec());
            } else if block[0] == b'O' {
                // Match optional extensions here; we currently have none
            } else if block[0] >= b'A' && block[0] <= b'Z' {
                // Match important extensions here; we currently have none
                // No match:
                //TODO: proper output of warnings
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
        assert_eq!( sum_reader.hash_bytes(), 32 );
        let mut sum32 = [0u8; 32];
        sum_reader.hash_result(&mut sum32);
        if buf32 != sum32 {
            return Err(Error::read("header checksum invalid", pos));
        }
        
        return Ok(FileHeader{
            name: repo_name,
            remarks: remarks,
            user_fields: user_fields
        });
        
        fn fill<R: io::Read>(r: &mut R, mut buf: &mut [u8], pos: usize) -> Result<()> {
            let mut p = pos;
            while buf.len() > 0 {
                match try!(r.read(buf)) {
                    0 => return Err(Error::read("corrupt (file terminates unexpectedly)", p)),
                    n => { buf = &mut mem::replace(&mut buf, &mut [])[n..]; p += n },
                }
            }
            Ok(())
        }
    }
    
    // "trim" applied to generic arrays: while the last char is v, remove it
    fn rtrim<T: cmp::PartialEq>(s: &[T], v: T) -> &[T] {
        let mut p = s.len();
        while p > 0 && s[p - 1] == v { p -= 1; }
        &s[0..p]
    }
    
    #[test]
    fn test_rtrim() {
        assert_eq!(rtrim(&[0, 15, 8], 15), &[0, 15, 8]);
        assert_eq!(rtrim(&[0, 15, 8, 8], 8), &[0, 15]);
        assert_eq!(rtrim(&[2.5], 2.5), &[]);
        assert_eq!(rtrim(&[], 'a'), &[]);
    }
    
    #[test]
    fn read_header() {
        // Note: checksum calculated with Python 3:
        // import hashlib
        // hashlib.sha256(b"PIPPINSS20150924...").digest()
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
        assert_eq!(header.user_fields, vec![b"Uuser rule\x00\x00\x00\x00\x00"]);
    }
}
