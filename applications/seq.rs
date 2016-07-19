// Subject to the ISC licence (LICENSE-ISC.txt).

//! Implement Pippin support for sequences (vectors). This includes code for
//! classification by length, and is probably not something you'd want to copy
//! directly into a real application.

use std::io::Write;
use std::cmp::min;
use std::u32;
use std::collections::hash_map::{HashMap, Entry};
use std::mem::size_of;

use byteorder::{ByteOrder, LittleEndian, ReadBytesExt, WriteBytesExt};

use pippin::*;
use pippin::repo::{ClassifyFallback, RepoDivideError};
use pippin::error::{ReadError, OtherError};


// —————  Sequence type itself  —————

/// Type of sequence elements.
pub type R = f64;
/// Type is a wrapper around a vector of f64. The reason for this is that we
/// can only implement ElementT for new types, thus cannot use the vector type
/// directly (see #44).
#[derive(PartialEq, Debug)]
pub struct Sequence {
    v: Vec<R>,
}
impl From<Vec<R>> for Sequence {
    fn from(v: Vec<R>) -> Self {
        Sequence { v: v }
    }
}

impl ElementT for Sequence {
    fn write_buf(&self, writer: &mut Write) -> Result<()> {
        for x in &self.v {
            try!(writer.write_f64::<LittleEndian>(*x));
        }
        Ok(())
    }
    fn read_buf(buf: &[u8]) -> Result<Self> {
        if buf.len() % size_of::<R>() != 0 {
            return OtherError::err("invalid data length");
        }
        let mut r: &mut &[u8] = &mut &buf[..];
        let n = buf.len() / size_of::<R>();
        let mut v = Vec::with_capacity(n);
        for _ in 0..n {
            v.push(try!(r.read_f64::<LittleEndian>()));
        }
        Ok(Sequence{ v: v })
    }
}


// —————  RepoT type and supporting types  —————

/// Data type implementing pippin's `ClassifierT` (stores information about
/// classifications).
#[derive(Clone)]
pub struct SeqClassifier {
    // For each class, the partition identifier and the min length of
    // sequences in the class. Ordered by min length, increasing.
    classes: Vec<(usize, PartId)>,
}
impl ClassifierT for SeqClassifier {
    type Element = Sequence;
    fn classify(&self, elt: &Sequence) -> Option<PartId> {
        let len = elt.v.len();
        match self.classes.binary_search_by(|x| x.0.cmp(&len)) {
            Ok(i) => Some(self.classes[i].1), // len equals lower bound
            Err(i) => {
                if i == 0 {
                    None    // shouldn't happen, since we should have a class with lower bound 0
                } else {
                    // i is index such that self.classes[i-1].0 < len < self.classes[i].0
                    Some(self.classes[i-1].1)
                }
            }
        }
    }
    fn fallback(&self) -> ClassifyFallback {
        // classify() only returns None if something is broken; stop
        ClassifyFallback::Fail
    }
}


/// Each classification has a PartId, a max PartId, a min length, a max length
/// and a version number. The PartId is stored as the key.
#[derive(Clone)]
pub struct PartInfo {
    max_part_id: PartId,
    // Information version; number increased each time partition changes
    ver: u32,
    min_len: u32,
    max_len: u32,
}

/// Type implementing pippin's `SeqRepo`.
pub struct SeqRepo<IO: RepoIO> {
    csf: SeqClassifier,
    io: IO,
    parts: HashMap<PartId, PartInfo>,
}
impl<IO: RepoIO> SeqRepo<IO> {
    /// Create an new repo around a given I/O device.
    pub fn new(r: IO) -> SeqRepo<IO> {
        SeqRepo {
            csf: SeqClassifier { classes: Vec::new() },
            io: r,
            parts: HashMap::new(),
        }
    }
    
    fn set_classifier(&mut self) {
        let mut classes = Vec::with_capacity(self.parts.len());
        for (part_id, part) in &self.parts {
            if part.max_len > part.min_len {
                classes.push((part.min_len as usize, part_id.clone()));
            }
        }
        // Note: there *could* be overlap of ranges. We can't do much if there
        // is and it won't cause failures later, so ignore this possibility.
        classes.sort_by(|a, b| a.0.cmp(&b.0));
        self.csf.classes = classes;
    }
    fn read_ud(v: &Vec<u8>) -> Result<(PartId, PartInfo), ReadError> {
        if v.len() != 32 {
            return Err(ReadError::new("incorrect length", 0, (0, v.len())));
        }
        if v[0..4] != *b"SCPI" {
            return Err(ReadError::new("unknown data (expected SCPI)", 0, (0, 4)));
        }
        let ver = LittleEndian::read_u32(&v[4..8]);
        let min_len = LittleEndian::read_u32(&v[8..12]);
        let max_len = LittleEndian::read_u32(&v[12..16]);
         let id = try_read!(PartId::try_from(LittleEndian::read_u64(&v[16..24])), 16, (0, 8));
        let max_id = try_read!(PartId::try_from(LittleEndian::read_u64(&v[24..32])), 24, (0, 8));
        let pi = PartInfo {
            max_part_id: max_id,
            ver: ver,
            min_len: min_len,
            max_len: max_len,
        };
        Ok((id, pi))
    }
}
impl<IO: RepoIO> UserFields for SeqRepo<IO> {
    fn write_user_fields(&mut self, _part_id: PartId, _is_log: bool) -> Vec<UserData> {
        let mut ud = Vec::with_capacity(self.parts.len());
        for (id,pi) in &self.parts {
            let mut buf = Vec::from(&b"SCPI...8..12..16..20..24..28..32"[..]);
            LittleEndian::write_u32(&mut buf[4..], pi.ver);
            LittleEndian::write_u32(&mut buf[8..], pi.min_len);
            LittleEndian::write_u32(&mut buf[12..], pi.max_len);
            LittleEndian::write_u64(&mut buf[16..], (*id).into());
            LittleEndian::write_u64(&mut buf[24..], pi.max_part_id.into());
            ud.push(UserData::Data(buf));
        }
        ud
    }
    fn read_user_fields(&mut self, user: Vec<UserData>, _part_id: PartId, _is_log: bool) {
        for ud in user {
            let (id, pi) = match ud {
                UserData::Data(v) => {
                    match Self::read_ud(&v) {
                        Ok(result) => result,
                        Err(e) => {
                            warn!("Error parsing user data: {}", e.display(&v));
                            continue;
                        },
                    }
                },
                UserData::Text(t) => {
                    warn!("Encounted user text: {}", t);
                    continue;
                },
            };
            match self.parts.entry(id) {
                Entry::Vacant(entry) => {
                    entry.insert(pi);
                },
                Entry::Occupied(entry) => {
                    if pi.ver > entry.get().ver {
                        let e = entry.into_mut();
                        e.max_part_id = pi.max_part_id;
                        e.ver = pi.ver;
                        e.min_len = pi.min_len;
                        e.max_len = pi.max_len;
                    }
                },
            }
        }
    }
}
impl<IO: RepoIO> RepoT<SeqClassifier> for SeqRepo<IO> {
    fn repo_io(&mut self) -> &mut RepoIO {
        &mut self.io
    }
    fn clone_classifier(&self) -> SeqClassifier {
        self.csf.clone()
    }
    fn init_first(&mut self) -> Result<Box<PartIO>> {
        assert!(self.parts.is_empty());
        let p_id = PartId::from_num(1);
        self.parts.insert(p_id, PartInfo {
            max_part_id: PartId::from_num(PartId::max_num()),
            ver: 0,
            min_len: 0,
            max_len: u32::MAX,
        });
        self.set_classifier();
        try!(self.io.new_part(p_id, ""));
        Ok(try!(self.io.make_part_io(p_id)))
    }
    fn divide(&mut self, part: &PartState<Sequence>) ->
        Result<(Vec<PartId>, Vec<PartId>), RepoDivideError>
    {
        // 1: choose new lengths to use for partitioning
        // Algorithm: sample up to 999 lengths, find the median
        if part.num_avail() < 1 { return Err(RepoDivideError::NotSubdivisible); }
        let mut lens = Vec::with_capacity(min(999, part.num_avail()));
        for elt in part.elt_map() {
            let seq: &Sequence = elt.1;
            assert!(seq.v.len() <= u32::MAX as usize);
            lens.push(seq.v.len() as u32);
            if lens.len() >= 999 { break; }
        }
        lens.sort();
        let mid_point = lens.len() / 2;
        let median = lens[mid_point];
        // 1st new class uses existing lower-bound; 2nd uses median as its lower bound
        
        // 2: find new partition numbers
        let old_id = part.part_id();
        let old_num = old_id.into_num();
        let (max_num, min_len, max_len) = match self.parts.get(&old_id) {
            Some(part) => 
                (part.max_part_id.into_num(), part.min_len, part.max_len),
            None => {
                return Err(RepoDivideError::msg("missing info"));
            },
        };
        if max_num < old_num + 2 {
            // Not enough numbers
            // TODO: steal numbers from other partitions
            return Err(RepoDivideError::NotSubdivisible);
        }
        let num1 = old_num + 1;
        let num2 = num1 + (max_num - old_num) / 2;
        let (id1, id2) = (PartId::from_num(num1), PartId::from_num(num2));
        
        // 3: update and report
        let ver = self.parts.get(&id1).map_or(0, |pi| pi.ver + 1);
        self.parts.insert(id1, PartInfo {
            max_part_id: PartId::from_num(num2 - 1),
            ver: ver,
            min_len: min_len,
            max_len: median - 1,
        });
        let ver = self.parts.get(&id2).map_or(0, |pi| pi.ver + 1);
        self.parts.insert(id2, PartInfo {
            max_part_id: PartId::from_num(max_num),
            ver: ver,
            min_len: median,
            max_len: max_len,
        });
        if let Some(pi) = self.parts.get_mut(&old_id) {
            pi.max_part_id = old_id;
            pi.ver = pi.ver + 1;
            pi.max_len = pi.min_len;    // mark as no longer in use
        }
        self.set_classifier();
        //TODO: what happens with return value?
        Ok((vec![id1, id2], vec![]))
    }
    fn write_buf(&self, _num: PartId, w: &mut Write) -> Result<()> {
        // Classifier data (little endian):
        // "SeqCSF01" identifier
        // Number of PartInfos (u32)
        // PartInfos: two PartIds (u64 × 2), ver (u32), lengths (u32 × 2)
        try!(w.write(b"SeqCSF01"));
        assert!(self.parts.len() <= u32::MAX as usize);
        try!(w.write_u32::<LittleEndian>(self.parts.len() as u32));
        for (part_id, part) in &self.parts {
            try!(w.write_u64::<LittleEndian>((*part_id).into()));
            try!(w.write_u64::<LittleEndian>(part.max_part_id.into()));
            try!(w.write_u32::<LittleEndian>(part.ver));
            try!(w.write_u32::<LittleEndian>(part.min_len));
            try!(w.write_u32::<LittleEndian>(part.max_len));
        }
        Ok(())
    }
    fn read_buf(&mut self, _num: PartId, buf: &[u8]) -> Result<()> {
        // Format is as defined in `write_buf()`.
        
        //TODO: how should errors be handled? Clean up handling.
        if buf.len() < 12 {
            return OtherError::err("Insufficient data for classifier's read_buf()");
        }
        if buf[0..8] != *b"SeqCSF01" {
            return OtherError::err("Invalid format identifier for classifier's read_buf()");
        }
        let n_parts = LittleEndian::read_u32(&buf[8..12]) as usize;
        if buf.len() != 12 + (8*2 + 4*3) * n_parts {
            return OtherError::err("Wrong data length for classifier's read_buf()");
        }
        let mut r = &mut &buf[12..];
        for _ in 0..n_parts {
            let part_id = try!(PartId::try_from(try!(r.read_u64::<LittleEndian>())));
            let max_part_id = try!(PartId::try_from(try!(r.read_u64::<LittleEndian>())));
            let ver = try!(r.read_u32::<LittleEndian>());
            let min_len = try!(r.read_u32::<LittleEndian>());
            let max_len = try!(r.read_u32::<LittleEndian>());
            
            match self.parts.entry(part_id) {
                Entry::Occupied(mut e) => {
                    if ver > e.get().ver {
                        // Replace all entries with more recent information
                        let v = e.get_mut();
                        v.max_part_id = max_part_id;
                        v.ver = ver;
                        v.min_len = min_len;
                        v.max_len = max_len;
                    }   // else: information is not newer; ignore
                },
                Entry::Vacant(e) => {
                    e.insert(PartInfo {
                        max_part_id: max_part_id,
                        ver: ver,
                        min_len: min_len,
                        max_len: max_len,
                    });
                },
            }
        }
        self.set_classifier();
        Ok(())
    }
}
