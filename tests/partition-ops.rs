//! Test Pippin operations on partitions
#![feature(box_syntax)]

extern crate pippin;
extern crate vec_map;

use std::io::{Read, Write, ErrorKind, stderr};
use std::path::Path;
use std::fs::{File, create_dir_all};
use std::any::Any;
use pippin::{Partition, PartitionIO, Element, Error, Result};
use vec_map::VecMap;

/// Allows writing to in-memory streams. Refers to external data so that it
/// can be recovered after the `Partition` is destroyed in the tests.
struct PartitionStreams {
    // Map of snapshot-number to pair (snapshot, map of log number to log)
    ss: VecMap<(Vec<u8>, VecMap<Vec<u8>>)>,
}

impl PartitionIO for PartitionStreams {
    fn as_any(&self) -> &Any { self }
    fn ss_len(&self) -> usize {
        self.ss.keys().next_back().map(|x| x+1).unwrap_or(0)
    }
    fn ss_cl_len(&self, ss_num: usize) -> usize {
        match self.ss.get(&ss_num) {
            Some(&(_, ref logs)) => logs.keys().next_back().map(|x| x+1).unwrap_or(0),
            None => 0,
        }
    }
    fn read_ss<'a>(&'a self, ss_num: usize) -> Result<Option<Box<Read+'a>>> {
        Ok(self.ss.get(&ss_num).map(|&(ref data, _)| box &data[..] as Box<Read+'a>))
    }
    fn read_ss_cl<'a>(&'a self, ss_num: usize, cl_num: usize) -> Result<Option<Box<Read+'a>>> {
        Ok(self.ss.get(&ss_num)
            .and_then(|&(_, ref logs)| logs.get(&cl_num))
            .map(|data| box &data[..] as Box<Read+'a>))
    }
    fn new_ss<'a>(&'a mut self, ss_num: usize) -> Result<Box<Write+'a>> {
        if self.ss.contains_key(&ss_num) {
            Err(Error::io(ErrorKind::AlreadyExists, "snapshot already exists"))
        } else {
            self.ss.insert(ss_num, (Vec::new(), VecMap::new()));
            return Ok(Box::new(&mut self.ss.get_mut(&ss_num).unwrap().0))
        }
    }
    fn append_ss_cl<'a>(&'a mut self, ss_num: usize, cl_num: usize) -> Result<Box<Write+'a>> {
        if let Some(data) = self.ss.get_mut(&ss_num).and_then(|&mut (_, ref mut logs)| logs.get_mut(&cl_num)) {
            let len = data.len();
            Ok(Box::new(&mut data[len..]))
        } else {
            Err(Error::io(ErrorKind::NotFound, "commit log not found"))
        }
    }
    fn new_ss_cl<'a>(&'a mut self, ss_num: usize, cl_num: usize) -> Result<Box<Write+'a>> {
        if let Some(&mut (_, ref mut logs)) = self.ss.get_mut(&ss_num) {
            if logs.contains_key(&cl_num) {
                return Err(Error::io(ErrorKind::AlreadyExists, "commit log already exists"));
            }
            logs.insert(cl_num, Vec::new());
            let data = logs.get_mut(&cl_num).unwrap();
            Ok(box &mut *data)
        } else {
            Err(Error::io(ErrorKind::NotFound, "no snapshot corresponding to new commit log"))
        }
    }
}

#[test]
fn create_small() {
    let part_streams = PartitionStreams { ss: VecMap::new() };
    let mut part = Partition::new(box part_streams).expect("creating partition");
    
    // 2 Add a few elements over multiple commits
    {
        let state = part.tip().expect("has tip");
        state.insert_elt(35, Element::from_str("thirty five")).expect("getting elt 35");
        state.insert_elt(6513, Element::from_str("six thousand, five hundred and thirteen"))
            .expect("getting elt 6513");
        state.insert_elt(5698131, Element::from_str(
            "five million, six hundred and ninety eight thousand, one hundred and thirty one"))
            .expect("getting elt 5698131");
    }
    part.commit().expect("committing");
    {
        let state = part.tip().expect("getting tip");
        state.insert_elt(68168, Element::from_str("sixty eight thousand, one hundred and sixty eight"))
            .expect("getting elt 68168");
    }
    part.commit().expect("committing");
    {
        let state = part.tip().expect("getting tip");
        state.insert_elt(89, Element::from_str("eighty nine")).expect("getting elt 89");
        state.insert_elt(1063, Element::from_str("one thousand and sixty three")).expect("getting elt 1063");
    }
    part.commit().expect("committing");
    
    // 3 Write to streams in memory
    part.write(true).expect("writing");
    
    let boxed_io = part.unwrap_io();
    let io = boxed_io.as_any().downcast_ref::<PartitionStreams>().expect("downcasting io");
    
    // 4 Compare streams to expected values
    assert_eq!(io.ss.len(), 1);
    assert!(io.ss.contains_key(&0));
    let &(ref ss_data, ref logs) = io.ss.get(&0).expect("io.ss.get(&0)");
    assert_eq!(logs.len(), 1);
    assert!(logs.contains_key(&0));
    let log = logs.get(&0).expect("logs.get(&0)");
    
    // This is a black-box test. We generate data and compare it to previous
    // versions; so long as it's the same we don't worry.
    // When formats change, we update the files, but keep the old version for
    // use in `read_small()` if backwards compatibility is to be maintained.
    
    let base_path = Path::new("tests/partition-ops");
    // We compare generated data to that found here:
    let path_read = base_path.join("v0.0.0");
    // When output differs, we write the result here (for further analysis):
    let path_write = base_path.join("output");
    let fname_ss0 = Path::new("partition-small-ss0.pip");
    let fname_ss0_cl0 = Path::new("partition-small-ss0-cl0.piplog");
    
    // Return Ok(true) if text is equal to that found in file
    let compare = |text: &[u8], fname: &Path| -> Result<bool> {
        println!("Reading: {}", path_read.join(fname).display());
        let mut f = try!(File::open(path_read.join(fname)));
        let mut buf = Vec::new();
        try!(f.read_to_end(&mut buf));
        let equal = *text == *buf;
        if !equal {
            let opath = path_write.join(fname);
            try!(writeln!(stderr(), "Files unequal; writing to {}", opath.display()));
            create_dir_all(&path_write).expect("create_dir_all");
            let mut of = try!(File::create(opath));
            try!(of.write(text));
        }
        Ok(*text == *buf)
    };
    
    // Read existing files and compare (or write when format changes)
    assert_eq!(compare(&ss_data, &fname_ss0).expect("comparing snapshot"), true);
    assert_eq!(compare(&log, &fname_ss0_cl0).expect("comparing commit log"), true);
}

// #[test]
// fn read_small() {
//     // TODO: Repeat this for each file version (to test backwards compatibility)
//     // 1 Read from file/fixed stream
//     // 2 Get latest state
//     // 3 Check list of all elements
//     // 4 Check a few individual elements
//     // 5 Repeat for some historical state
//     assert!(false);
// }
