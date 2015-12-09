//! Test Pippin operations on partitions
#![feature(box_syntax)]

extern crate pippin;
extern crate vec_map;

use std::mem::transmute;
use std::io::{Read, Write, ErrorKind};
use std::fs::File;
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
            return Ok(box &mut self.ss.get_mut(&ss_num).unwrap().0[..])
        }
    }
    fn append_ss_cl<'a>(&'a mut self, ss_num: usize, cl_num: usize) -> Result<Box<Write+'a>> {
        match self.ss.get_mut(&ss_num).and_then(|&mut (_, ref mut logs)| logs.get_mut(&cl_num)) {
            Some(data) => {
                let len = data.len();
                Ok(box &mut data[len..])
            },
            None => Err(Error::io(ErrorKind::NotFound, "commit log not found"))
        }
    }
    fn new_ss_cl<'a>(&'a mut self, ss_num: usize, cl_num: usize) -> Result<Box<Write+'a>> {
        if let Some(&mut (_, ref mut logs)) = self.ss.get_mut(&ss_num) {
            if logs.contains_key(&cl_num) {
                return Err(Error::io(ErrorKind::AlreadyExists, "commit log already exists"));
            }
            logs.insert(cl_num, Vec::new());
            let data = logs.get_mut(&cl_num).unwrap();
            Ok(box &mut data[..])
        } else {
            Err(Error::io(ErrorKind::NotFound, "no snapshot corresponding to new commit log"))
        }
    }
}

#[test]
fn create_small() {
        let mut part_streams = PartitionStreams { ss: VecMap::new() };
        let mut part = Partition::create(box part_streams).new();
        
        // 2 Add a few elements over multiple commits
        {
            let state = part.tip().expect("has tip");
            state.insert_elt(35, Element::from_str("thirty five")).expect("success");
            state.insert_elt(6513, Element::from_str("six thousand, five hundred and thirteen")).expect("success");
            state.insert_elt(5698131, Element::from_str("five million, six hundred and ninety eight thousand, one hundred and thirty one")).expect("success");
        }
        part.commit().expect("success");
        {
            let state = part.tip().expect("has tip");
            state.insert_elt(68168, Element::from_str("sixty eight thousand, one hundred and sixty eight")).expect("success");
        }
        part.commit().expect("success");
        {
            let state = part.tip().expect("has tip");
            state.insert_elt(89, Element::from_str("eighty nine")).expect("success");
            state.insert_elt(1063, Element::from_str("one thousand and sixty three")).expect("success");
        }
        part.commit().expect("success");
        
        // 3 Write to streams in memory
        part.write(true).expect("success");
    
    let boxed_io = part.unwrap_io();
    let io = (&boxed_io as &Any).downcast_ref::<PartitionStreams>().unwrap();
    
    // 4 Compare streams to expected values (from files?)
    assert_eq!(io.ss.len(), 1);
    assert!(io.ss.contains_key(&1));
    let &(ref ss_data, ref logs) = io.ss.get(&1).unwrap();
    assert_eq!(logs.len(), 3);
    assert!(logs.contains_key(&1) && logs.contains_key(&2) && logs.contains_key(&3));
    let (log_1, log_2, log_3) = (logs.get(&1).unwrap(), logs.get(&2).unwrap(), logs.get(&3).unwrap());
    assert!(write(&ss_data, "partition-small-ss1.pip").is_ok());
    assert!(write(log_1, "partition-small-ss1-cl0.pipl").is_ok());
    assert!(write(log_2, "partition-small-ss1-cl1.pipl").is_ok());
    assert!(write(log_3, "partition-small-ss1-cl2.pipl").is_ok());
    
    fn write(text: &[u8], filename: &str) -> Result<()> {
        let mut f = try!(File::create(filename));
        try!(f.write(text));
        Ok(())
    }
}

#[test]
fn read_small() {
    // 1 Read from file/fixed stream
    // 2 Get latest state
    // 3 Check list of all elements
    // 4 Check a few individual elements
    // 5 Repeat for some historical state
    assert!(false);
}
