/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Test Pippin operations on partitions

extern crate pippin;
extern crate vec_map;
extern crate log;
extern crate env_logger;

use std::io::{Read, Write, ErrorKind};

use vec_map::VecMap;

use pippin::pip::*;

type Data = Vec<u8>;

/// Allows writing to in-memory streams. Refers to external data so that it
/// can be recovered after the `Partition` is destroyed in the tests.
#[derive(Debug)]
struct PartitionStreams {
    // Map of snapshot-number to pair (snapshot, map of log number to log)
    ss: VecMap<(Option<Data>, VecMap<Data>)>,
}

impl PartIO for PartitionStreams {
    fn ss_len(&self) -> usize {
        self.ss.keys().next_back().map(|x| x+1).unwrap_or(0)
    }
    fn ss_cl_len(&self, ss_num: usize) -> usize {
        match self.ss.get(ss_num) {
            Some(&(_, ref logs)) => logs.keys().next_back().map(|x| x+1).unwrap_or(0),
            None => 0,
        }
    }
    fn has_ss(&self, ss_num: usize) -> bool {
        self.ss.get(ss_num).map(|&(ref ss, _)| ss.is_some()).unwrap_or(false)
    }
    fn read_ss<'a>(&'a self, ss_num: usize) -> Result<Option<Box<Read+'a>>> {
        Ok(self.ss.get(ss_num)
                .and_then(|&(ref ss, _)| 
                    ss.as_ref().map(|data| Box::new(&data[..]) as Box<Read+'a>)))
    }
    fn read_ss_cl<'a>(&'a self, ss_num: usize, cl_num: usize) -> Result<Option<Box<Read+'a>>> {
        Ok(self.ss.get(ss_num)
            .and_then(|&(_, ref logs)| logs.get(cl_num))
            .map(|data| Box::new(&data[..]) as Box<Read+'a>))
    }
    fn new_ss<'a>(&'a mut self, ss_num: usize) -> Result<Option<Box<Write+'a>>> {
        {
            let pair = self.ss.entry(ss_num).or_insert((None, VecMap::new()));
            if pair.0 == None {
                pair.0 = Some(Vec::new());
            } else {
                return Ok(None);
            }
        }
        let data: &'a mut Data = (&mut self.ss.get_mut(ss_num).unwrap().0).as_mut().unwrap();
        Ok(Some(Box::new(data)))
    }
    fn append_ss_cl<'a>(&'a mut self, ss_num: usize, cl_num: usize) -> Result<Option<Box<Write+'a>>> {
        if let Some(data) = self.ss.get_mut(ss_num).and_then(|&mut (_, ref mut logs)| logs.get_mut(cl_num)) {
            let len = data.len();
            Ok(Some(Box::new(&mut data[len..])))
        } else {
            Ok(None)
        }
    }
    fn new_ss_cl<'a>(&'a mut self, ss_num: usize, cl_num: usize) -> Result<Option<Box<Write+'a>>> {
        if let Some(&mut (_, ref mut logs)) = self.ss.get_mut(ss_num) {
            if logs.contains_key(cl_num) {
                return Ok(None);
            }
            logs.insert(cl_num, Vec::new());
            let data = logs.get_mut(cl_num).unwrap();
            Ok(Some(Box::new(&mut *data)))
        } else {
            make_io_err(ErrorKind::NotFound, "no snapshot corresponding to new commit log")
        }
    }
}

#[test]
fn create_small() {
    type Control = DefaultControl<String, PartitionStreams>;
    
    env_logger::init().unwrap();
    
    let part_streams = PartitionStreams { ss: VecMap::new() };
    let control = Control::new(part_streams);
    let mut part = Partition::create(control, "create_small")
            .expect("creating partition");
    
    // 2 Add a few elements over multiple commits
    let mut state = part.tip().expect("has tip").clone_mut();
    state.insert_new("thirty five".to_string()).expect("inserting elt 35");
    state.insert_new("six thousand, five hundred and thirteen"
            .to_string()).expect("inserting elt 6513");
    state.insert_new("five million, six hundred and ninety eight \
            thousand, one hundred and thirty one".to_string())
            .expect("inserting elt 5698131");
    part.push_state(state).expect("committing");
    let state1 = part.tip().expect("has tip").clone_exact();
    
    let mut state = part.tip().expect("getting tip").clone_mut();
    state.insert_new("sixty eight thousand, one hundred and sixty eight"
            .to_string()).expect("inserting elt 68168");
    part.push_state(state).expect("committing");
    
    let mut state = part.tip().expect("getting tip").clone_mut();
    state.insert_new("eighty nine".to_string()).expect("inserting elt 89");
    state.insert_new("one thousand and sixty three".to_string())
            .expect("inserting elt 1063");
    part.push_state(state).expect("committing");
    let state3 = part.tip().expect("has tip").clone_exact();
    
    // 3 Write to streams in memory
    part.write_fast().expect("writing");
    let control = part.unwrap_control();
    
    // 4 Check the generated streams
    {
        let io = control.io();
        assert_eq!(io.ss.len(), 1);
        assert!(io.ss.contains_key(0));
        let &(ref ss_data, ref logs) = io.ss.get(0).expect("io.ss.get(0)");
        assert_eq!(logs.len(), 1);
        assert!(logs.contains_key(0));
        let log = logs.get(0).expect("logs.get(0)");
        
        // It is sometimes useful to be able to see these streams. This can be
        // done here:
        /*
        use std::path::Path;
        use std::io::stderr;
        use std::fs::{File, create_dir_all};
        let out_path = Path::new("output/partition-ops");
        let fname_ss0 = Path::new("partition-small-ss0.pip");
        let fname_ss0_cl0 = Path::new("partition-small-ss0-cl0.piplog");
        let write_out = |text: &[u8], fname: &Path| -> Result<()> {
            let opath = out_path.join(fname);
            try!(writeln!(stderr(), "Writing create_small() test output to {}", opath.display()));
            create_dir_all(&out_path).expect("create_dir_all");
            let mut of = try!(File::create(opath));
            try!(of.write(text));
            Ok(())
        };
        write_out(&ss_data, &fname_ss0).expect("writing snapshot file");
        write_out(&log, &fname_ss0_cl0).expect("writing commit log");
        */
        
        // We cannot do a binary comparison on the output files since the order
        // in which elements occur can and does vary (thanks to Rust's hash
        // function randomisation). Instead we compare file length here and
        // read the files back below.
        assert_eq!(ss_data.as_ref().map_or(0, |d| d.len()), 208);
        assert_eq!(log.len(), 1168);
    }
    
    // 5 Read streams back again and compare
    let mut part2 = Partition::open(control, true).expect("opening partition");
    part2.load_all().expect("part2.load");
    assert_eq!(state1,
        *part2.state(state1.statesum()).expect("get state1 by sum"));
    assert_eq!(state3, *part2.tip().expect("part2 tip"));
}
