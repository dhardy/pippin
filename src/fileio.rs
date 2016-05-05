/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Pippin: file access for repositories and partitions.

use std::path::{Path, PathBuf};
use std::io::{Read, Write};
use std::fs::{File, OpenOptions};
use std::any::Any;
use std::ops::Add;

use vec_map::{VecMap, Entry};

use {PartIO, PartId};
use error::{Result};


// —————  Partition  —————

/// Remembers a set of file names associated with a partition, opens read
/// and write streams on these and creates new partition files.
#[derive(Debug)]
pub struct PartFileIO {
    // Partition identifier (required)
    part_id: PartId,
    // Appended with snapshot/log number and extension to get a file path
    prefix: PathBuf,
    // First key is snapshot number. Value is (if found) a path to the snapshot
    // file and a map of log files.
    // Key of internal map is log number. Value is a path to the log file.
    ss: VecMap<(Option<PathBuf>, VecMap<PathBuf>)>,
}

impl PartFileIO {
    /// Create an empty partition IO. This is equivalent to calling `new` with
    /// `VecMap::new()` as the third argument.
    /// 
    /// *   `part_id` is the partition identifier
    /// *   `prefix` is a dir + partial-file-name; it is appended with
    ///     something like `-ss1.pip` or `-ss2-lf3.piplog` to get a file name
    pub fn new_empty(part_id: PartId, prefix: PathBuf) -> PartFileIO {
        Self::new(part_id, prefix, VecMap::new())
    }
    
    /// Create a partition IO with paths to some existing files.
    /// 
    /// *   `part_id` is the partition identifier
    /// *   `prefix` is a dir + partial-file-name; it is appended with
    ///     something like `-ss1.pip` or `-ss2-lf3.piplog` to get a file name
    /// *   `paths` is a list of paths of all known partition files
    pub fn new(part_id: PartId, prefix: PathBuf,
        paths: VecMap<(Option<PathBuf>, VecMap<PathBuf>)>) -> PartFileIO
    {
        PartFileIO {
            part_id: part_id,
            prefix: prefix,
            ss: paths,
        }
    }
    
    /// Get the number of snapshot numbers found. Note: usually each number has
    /// a snapshot file, but this is not guaranteed (e.g. if the file is lost
    /// but logs are found).
    pub fn len_ss(&self) -> usize {
        self.ss.len()
    }
    
    /// Count the snapshot files present.
    pub fn num_ss_files(&self) -> usize {
        self.ss.values().filter(|v| v.0.is_some()).count()
    }
    
    /// Count the log files present.
    pub fn num_cl_files(&self) -> usize {
        // #0018: could use `.sum()` but see https://github.com/rust-lang/rust/issues/27739
        self.ss.values().map(|v| v.1.len()).fold(0, Add::add)
    }
    
    /// Returns a reference to the path of a snapshot file, if found.
    pub fn get_ss_path(&self, ss: usize) -> Option<&Path> {
        self.ss.get(ss).and_then(|&(ref p, _)|
            p.as_ref().map(|ref path| path.as_path()))
    }
    
    /// Returns a reference to the path of a log file, if found.
    pub fn get_cl_path(&self, ss: usize, cl: usize) -> Option<&Path> {
        self.ss.get(ss)     // Option<(_, VecMap<PathBuf>)>
            .and_then(|&(_, ref logs)| logs.get(cl))    // Option<PathBuf>
            .map(|p| p.as_path())
    }
}

impl PartIO for PartFileIO {
    fn as_any(&self) -> &Any { self }
    
    fn part_id(&self) -> PartId { self.part_id }
    
    fn ss_len(&self) -> usize {
        self.ss.keys().next_back().map(|x| x+1).unwrap_or(0)
    }
    fn ss_cl_len(&self, ss_num: usize) -> usize {
        self.ss.get(ss_num) // Option<(_, VecMap<PathBuf>)>
            .and_then(|&(_, ref logs)| logs.keys().next_back())
            .map(|x| x+1).unwrap_or(0)
    }
    
    fn read_ss<'a>(&self, ss_num: usize) -> Result<Option<Box<Read+'a>>> {
        // Cannot replace `match` with `map` since `try!()` cannot be used in a closure
        Ok(match self.ss.get(ss_num) {
            Some(&(ref p, _)) => {
                if let &Some(ref path) = p {
                    trace!("Reading snapshot file: {}", path.display());
                    Some(box try!(File::open(path)))
                } else {
                    None
                }
            },
            None => None
        })
    }
    
    fn read_ss_cl<'a>(&self, ss_num: usize, cl_num: usize) -> Result<Option<Box<Read+'a>>> {
        Ok(match self.ss.get(ss_num).and_then(|&(_, ref logs)| logs.get(cl_num)) {
            Some(p) => {
                trace!("Reading log file: {}", p.display());
                Some(box try!(File::open(p)))
            },
            None => None,
        })
    }
    
    fn new_ss<'a>(&mut self, ss_num: usize) -> Result<Option<Box<Write+'a>>> {
        let mut p = self.prefix.as_os_str().to_os_string();
        p.push(format!("-ss{}.pip", ss_num));
        let p = PathBuf::from(p);
        if self.ss.get(ss_num).map_or(false, |&(ref p, _)| p.is_some()) || p.exists() {
            // File already exists in internal map or on filesystem
            return Ok(None);
        }
        trace!("Creating snapshot file: {}", p.display());
        let stream = try!(File::create(&p));
        match self.ss.entry(ss_num) {
            Entry::Occupied(mut entry) => { entry.get_mut().0 = Some(p); },
            Entry::Vacant(entry) => { entry.insert((Some(p), VecMap::new())); },
        };
        Ok(Some(box stream))
    }
    
    fn append_ss_cl<'a>(&mut self, ss_num: usize, cl_num: usize) -> Result<Option<Box<Write+'a>>> {
        Ok(match self.ss.get(ss_num).and_then(|&(_, ref logs)| logs.get(cl_num)) {
            Some(p) => {
                trace!("Appending to log file: {}", p.display());
                Some(box try!(OpenOptions::new().write(true).append(true).open(p)))
            },
            None => None
        })
    }
    fn new_ss_cl<'a>(&mut self, ss_num: usize, cl_num: usize) -> Result<Option<Box<Write+'a>>> {
        let mut logs = &mut self.ss.entry(ss_num).or_insert_with(|| (None, VecMap::new())).1;
        let mut p = self.prefix.as_os_str().to_os_string();
        p.push(format!("-ss{}-cl{}.piplog", ss_num, cl_num));
        let p = PathBuf::from(p);
        if logs.contains_key(cl_num) || p.exists() {
            // File already exists in internal map or on filesystem
            return Ok(None);
        }
        trace!("Creating log file: {}", p.display());
        let stream = try!(OpenOptions::new().create(true).write(true).append(true).open(&p));
        logs.insert(cl_num, p);
        Ok(Some(box stream))
    }
}
