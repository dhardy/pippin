//! Pippin: file discovery

use std::path::{Path, PathBuf};
use std::fs::PathExt;
use std::io::{Read, Write, ErrorKind};
use std::fs::{read_dir, File, OpenOptions};
use std::cmp::max;
use regex::Regex;
use vec_map::VecMap;

use super::partition::PartitionIO;
use error::{Result, Error};


struct PartitionFiles {
    base_path: String,  // this is path + first part of file name
    len: usize, // largest index in snapshots + 1
    snapshots: VecMap<PathBuf>,
    logs: VecMap<VecMap<PathBuf>>,
}

impl PartitionFiles {
    /// Create a new instance
    pub fn from_dir_basename(path: &Path, basename: &str) -> Result<PartitionFiles> {
        if !path.is_dir() { return Err(Error::arg("path is not a directory")); }
        
        let ss_pat = try!(Regex::new("-ss([0-9]+).pip"));
        let cl_pat = try!(Regex::new("-ss([0-9]+)-cl([0-9]+).pipl"));
        let blen = basename.len();
        
        let mut snapshots = VecMap::new();
        let mut logs = VecMap::new();
        
        for entry in try!(read_dir(path)) {
            let entry = try!(entry);
            let os_fname = entry.file_name();
            let fname = match os_fname.to_str() {
                Some(s) => s,
                None => {
                    // TODO: warn that name was unmappable
                    continue;
                }
            };
            if fname[0..blen] != *basename {
                continue;   // no match
            }
            if let Some(caps) = ss_pat.captures(&fname[blen..]) {
                let ss: usize = try!(caps.at(1).expect("match should yield capture").parse());
                if let Some(replaced) = snapshots.insert(ss, entry.path()) {
                    return Err(Error::repo_files(format!(
                        "partition files have same number:\n  {}\n  {}", replaced.display(), entry.path().display())));
                }
            } else if let Some(caps) = cl_pat.captures(&fname[blen..]) {
                let ss: usize = try!(caps.at(1).expect("match should yield capture").parse());
                let cl: usize = try!(caps.at(2).expect("match should yield capture").parse());
                let s_vec = &mut logs.entry(ss).or_insert_with(|| VecMap::new());
                if let Some(replaced) = s_vec.insert(cl, entry.path()) {
                    return Err(Error::repo_files(format!(
                        "partition files have same number:\n  {}\n  {}", replaced.display(), entry.path().display())));
                }
            } // else: no match; ignore
        }
        
        let base_path = path.to_str().unwrap().to_string()+ basename;  // TODO: optimise
        let len = max(snapshots.keys().last().unwrap_or(0), logs.keys().last().unwrap_or(0));
        Ok(PartitionFiles { base_path: base_path, len: len, snapshots: snapshots, logs: logs })
    }
    
    fn getp_ss_cl(&self, ss_num: usize, cl_num: usize) -> Option<&PathBuf> {
        self.logs.get(&ss_num).map_or(None, |logs| logs.get(&cl_num))
    }
}

impl PartitionIO for PartitionFiles {
    fn ss_len(&self) -> usize { self.len }
    fn ss_cl_len(&self, ss_num: usize) -> usize {
        match self.logs.get(&ss_num) {
            Some(logs) => logs.keys().last().unwrap_or(0),
            None => 0,
        }
    }
    
    fn read_ss(&self, ss_num: usize) -> Result<Option<Box<Read>>> {
        Ok(match self.snapshots.get(&ss_num) {
            Some(p) => Some(box try!(File::open(p))),
            None => None,
        })
    }
    
    fn read_ss_cl(&self, ss_num: usize, cl_num: usize) -> Result<Option<Box<Read>>> {
        Ok(match self.getp_ss_cl(ss_num, cl_num) {
            Some(p) => Some(box try!(File::open(p))),
            None => None,
        })
    }
    
    fn new_ss(&mut self, ss_num: usize) -> Result<Box<Write>> {
        if !self.snapshots.contains_key(&ss_num) {
            let p = PathBuf::from(format!("{}-ss{}.pip", self.base_path, self.len));
            if !p.exists() {
                // TODO: atomic if-exists-don't-overwrite
                let stream = try!(File::create(&p));
                self.len = ss_num + 1;
                self.snapshots.insert(ss_num, p);
                return Ok(box stream)
            }
        }
        Err(Error::io(ErrorKind::AlreadyExists, "snapshot file already exists"))
    }
    
    fn append_ss_cl(&mut self, ss_num: usize, cl_num: usize) -> Result<Box<Write>> {
        match self.getp_ss_cl(ss_num, cl_num) {
            Some(p) => {
                let stream = try!(OpenOptions::new().write(true).append(true).open(p));
                Ok(box stream)
            },
            None => Err(Error::io(ErrorKind::NotFound, "commit log file not found"))
        }
    }
    fn new_ss_cl(&mut self, ss_num: usize, cl_num: usize) -> Result<Box<Write>> {
        if self.getp_ss_cl(ss_num, cl_num) == None {
            let p = PathBuf::from(format!("{}-ss{}-cl{}.pip", self.base_path, ss_num, cl_num));
            if !p.exists() {
                let stream = try!(OpenOptions::new().create(true).write(true).append(true).open(p));
                return Ok(box stream)
            } else {
                // p exists but is not in our path cache; add (but still fail with the err below)
                self.logs.entry(ss_num).or_insert_with(|| VecMap::new()).insert(cl_num, p);
                //TODO: update with new data??
            }
        }
        Err(Error::io(ErrorKind::AlreadyExists, "commit log file already exists"))
    }
}
