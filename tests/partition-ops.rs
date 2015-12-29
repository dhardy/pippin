//! Test Pippin operations on partitions
#![feature(box_syntax)]

extern crate pippin;
extern crate vec_map;

use std::io::{Read, Write, ErrorKind};
use std::any::Any;
// Used to write results out:
// use std::io::stderr;
// use std::path::Path;
// use std::fs::{File, create_dir_all};
use pippin::{Partition, PartitionIO, Element};
use pippin::error::{make_io_err, Result};
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
            make_io_err(ErrorKind::AlreadyExists, "snapshot already exists")
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
            make_io_err(ErrorKind::NotFound, "commit log not found")
        }
    }
    fn new_ss_cl<'a>(&'a mut self, ss_num: usize, cl_num: usize) -> Result<Box<Write+'a>> {
        if let Some(&mut (_, ref mut logs)) = self.ss.get_mut(&ss_num) {
            if logs.contains_key(&cl_num) {
                return make_io_err(ErrorKind::AlreadyExists, "commit log already exists");
            }
            logs.insert(cl_num, Vec::new());
            let data = logs.get_mut(&cl_num).unwrap();
            Ok(box &mut *data)
        } else {
            make_io_err(ErrorKind::NotFound, "no snapshot corresponding to new commit log")
        }
    }
}

#[test]
fn create_small() {
    let part_streams = PartitionStreams { ss: VecMap::new() };
    let mut part = Partition::new(box part_streams, "create_small").expect("creating partition");
    
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
    let state1 = part.tip().expect("has tip").clone();
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
    let state3 = part.tip().expect("has tip").clone();
    
    // 3 Write to streams in memory
    part.write(true).expect("writing");
    let boxed_io = part.unwrap_io();
    
    // 4 Check the generated streams
    {
        let io = boxed_io.as_any().downcast_ref::<PartitionStreams>().expect("downcasting io");
        assert_eq!(io.ss.len(), 1);
        assert!(io.ss.contains_key(&0));
        let &(ref ss_data, ref logs) = io.ss.get(&0).expect("io.ss.get(&0)");
        assert_eq!(logs.len(), 1);
        assert!(logs.contains_key(&0));
        let log = logs.get(&0).expect("logs.get(&0)");
        
        // It is sometimes useful to be able to see these streams. This can be
        // done here (you also need to uncomment "use" statements above):
//         let out_path = Path::new("output/partition-ops");
//         let fname_ss0 = Path::new("partition-small-ss0.pip");
//         let fname_ss0_cl0 = Path::new("partition-small-ss0-cl0.piplog");
//         let write_out = |text: &[u8], fname: &Path| -> Result<()> {
//             let opath = out_path.join(fname);
//             try!(writeln!(stderr(), "Writing create_small() test output to {}", opath.display()));
//             create_dir_all(&out_path).expect("create_dir_all");
//             let mut of = try!(File::create(opath));
//             try!(of.write(text));
//             Ok(())
//         };
//         write_out(&ss_data, &fname_ss0).expect("writing snapshot file");
//         write_out(&log, &fname_ss0_cl0).expect("writing commit log");
        
        // We cannot do a binary comparison on the output files since the order
        // in which elements occur can and does vary (thanks to Rust's hash
        // function randomisation). Instead we compare file length here and
        // read the files back below.
        assert_eq!(ss_data.len(), 192);
        assert_eq!(log.len(), 1120);
    }
    
    // 5 Read streams back again and compare
    let mut part2 = Partition::create(boxed_io);
    part2.load(true).expect("part2.load");
    assert_eq!(state1, *part2.state(state1.statesum_ref()).expect("get state1 by sum"));
    assert_eq!(state3, *part2.tip().expect("part2 tip"));
}
