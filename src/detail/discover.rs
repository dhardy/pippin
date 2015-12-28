//! Pippin: file discovery

use std::path::{Path, PathBuf};
use std::io::{Read, Write, ErrorKind};
use std::fs::{read_dir, File, OpenOptions};
use std::any::Any;
use regex::Regex;
use vec_map::VecMap;

use super::partition::PartitionIO;
use error::{Result, Error, PathError, make_io_err};


/// A helper to find files belonging to a partition (assuming a standard
/// layout on a local or mapped filesystem) and provide access.
/// 
/// As an alternative, users could provide their own implementations of
/// PartitionIO.
pub struct DiscoverPartitionFiles {
    dir: PathBuf,
    basename: String,  // first part of file name
    // Map of snapshot-number to pair (snapshot, map of log number to log)
    // The snapshot path may be empty (if not found).
    ss: VecMap<(PathBuf, VecMap<PathBuf>)>,
}

impl DiscoverPartitionFiles {
    /// Create a new instance.
    /// 
    /// `path` must be a directory containing (or in the case of a new repo, to
    /// contain) data files for the existing partition. `basename` is the first
    /// part of the file name, common to all files of this partition.
    pub fn from_dir_basename(path: &Path, basename: &str) -> Result<DiscoverPartitionFiles> {
        if !path.is_dir() { return PathError::err("not a directory", path.to_path_buf()); }
        //TODO: validate basename
        
        let ss_pat = try!(Regex::new("-ss([1-9][0-9]*).pip"));
        let cl_pat = try!(Regex::new("-ss([1-9][0-9]*)-cl([1-9][0-9]*).pipl"));
        let blen = basename.len();
        
        let mut snapshots = VecMap::new();
        
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
                if let Some(_replaced) = snapshots.insert(ss, (entry.path(), VecMap::new())) {
                    panic!("multiple files map to same basname/number");
                }
            } else if let Some(caps) = cl_pat.captures(&fname[blen..]) {
                let ss: usize = try!(caps.at(1).expect("match should yield capture").parse());
                let cl: usize = try!(caps.at(2).expect("match should yield capture").parse());
                let s_vec = &mut snapshots.entry(ss).or_insert_with(|| (PathBuf::new(), VecMap::new()));
                if let Some(_replaced) = s_vec.1.insert(cl, entry.path()) {
                    panic!("multiple files map to same basname/number");
                }
            } // else: no match; ignore
        }
        
        Ok(DiscoverPartitionFiles {
            dir: path.to_path_buf(),
            basename: basename.to_string(),
            ss: snapshots })
    }
    
    /// Create a new instance, loading only those paths given. Each path must
    /// be a Pippin file. 
    /// 
    /// Directory and base-name for files are taken from the first path given.
    pub fn from_paths(paths: Vec<PathBuf>) -> Result<DiscoverPartitionFiles> {
        //TODO: allowable charaters in basename
        let ss_pat = try!(Regex::new(r"([0-9a-zA-Z-_]+)-ss(0|[1-9][0-9]*).pip"));
        let cl_pat = try!(Regex::new(r"([0-9a-zA-Z-_]+)-ss(0|[1-9][0-9]*)-cl(0|[1-9][0-9]*).pipl"));
        
        let mut snapshots = VecMap::new();
        let mut dir_path = None;
        let mut basename = None;
        
        for path in paths.into_iter() {
            if !path.is_file() {
                return PathError::err("not a file", path);
            }
            if dir_path == None {
                dir_path = Some(path.parent().expect("all file paths should have a parent").to_path_buf());
            }
            enum FileIs {
                SnapShot(usize),
                CommitLog(usize, usize),
                BadFileName(&'static str),
            }
            let file_is = {
                // Within this block we borrow from `path`, so the borrow checker will not us
                // move `path`. (A more precise checker might make allow this.)
                if let Some(fname) = path.file_name().expect("file path must have a file name").to_str() {
                    if let Some(caps) = ss_pat.captures(fname) {
                        if basename == None {
                            basename = Some(caps.at(1).expect("match should yield capture").to_string());
                        }
                        let ss: usize = try!(caps.at(2).expect("match should yield capture").parse());
                        FileIs::SnapShot(ss)
                    } else if let Some(caps) = cl_pat.captures(fname) {
                        if basename == None {
                            basename = Some(caps.at(1).expect("match should yield capture").to_string());
                        }
                        let ss: usize = try!(caps.at(2).expect("match should yield capture").parse());
                        let cl: usize = try!(caps.at(3).expect("match should yield capture").parse());
                        FileIs::CommitLog(ss, cl)
                    } else {
                        if fname.ends_with(".pip") {
                            FileIs::BadFileName("Snapshot file names should have form BASENAME-ssNUM.pip")
                        } else if fname.ends_with(".pipl") {
                            FileIs::BadFileName("Commit log file names should have form BASENAME-ssNUM-clNUM.pipl")
                        } else {
                            FileIs::BadFileName("Not a Pippin file (name doesn't end .pip or .pipl")
                        }
                    }
                } else {
                    FileIs::BadFileName("could not convert file name to unicode")
                }
            };
            match file_is {
                // Decisions made. Now we can move path without worrying the borrow checker.
                FileIs::SnapShot(ss) => {
                    if let Some(_replaced) = snapshots.insert(ss, (path, VecMap::new())) {
                        panic!("multiple files map to same basename/number");
                    }
                },
                FileIs::CommitLog(ss, cl) => {
                    let s_vec = &mut snapshots.entry(ss).or_insert_with(|| (PathBuf::new(), VecMap::new()));
                    if let Some(_replaced) = s_vec.1.insert(cl, path) {
                        panic!("multiple files map to same basename/number");
                    }
                },
                FileIs::BadFileName(msg) => {
                    return PathError::err(msg, path);
                },
            }
        }
        
        if basename == None {
            return make_io_err(ErrorKind::NotFound, "no path");
        }
        Ok(DiscoverPartitionFiles {
            dir: dir_path.expect("dir_path should be set when basename is set"),
            basename: basename.unwrap(/*tested above*/),
            ss: snapshots })
    }
    
    /// Output the number of snapshot files found.
    /// 
    /// Actually, this includes snapshot numbers with logs but no snapshot.
    /// API not fixed.
    pub fn num_ss_files(&self) -> usize {
        self.ss.len()
    }
    
    /// Output the number of log files found.
    /// 
    /// API not fixed.
    pub fn num_cl_files(&self) -> usize {
        let mut num = 0;
        for &(_, ref logs) in self.ss.values() {
            num += logs.len();
        }
        num
    }
    
    /// Returns a reference to the path of a snapshot file, if found.
    pub fn get_ss_path(&self, ss: usize) -> Option<&Path> {
        self.ss.get(&ss).and_then(|&(ref p, _)| if *p == PathBuf::new() { None } else { Some(p.as_path()) })
    }
    
    /// Returns a reference to the path of a log file, if found.
    pub fn get_cl_path(&self, ss: usize, cl: usize) -> Option<&Path> {
        self.ss.get(&ss)
            .and_then(|&(_, ref logs)| logs.get(&cl))
            .map(|p| p.as_path())
    }
}

impl PartitionIO for DiscoverPartitionFiles {
    fn as_any(&self) -> &Any { self }
    
    fn ss_len(&self) -> usize {
        self.ss.keys().next_back().map(|x| x+1).unwrap_or(0)
    }
    fn ss_cl_len(&self, ss_num: usize) -> usize {
        self.ss.get(&ss_num)
            .and_then(|&(_, ref logs)| logs.keys().next_back())
            .map(|x| x+1).unwrap_or(0)
    }
    
    fn read_ss<'a>(&self, ss_num: usize) -> Result<Option<Box<Read+'a>>> {
        // Cannot replace `match` with `map` since `try!()` cannot be used in a closure
        Ok(match self.ss.get(&ss_num) {
            Some(&(ref p, _)) => Some(box try!(File::open(p))),
            None => None
        })
    }
    
    fn read_ss_cl<'a>(&self, ss_num: usize, cl_num: usize) -> Result<Option<Box<Read+'a>>> {
        Ok(match self.ss.get(&ss_num).and_then(|&(_, ref logs)| logs.get(&cl_num)) {
            Some(p) => Some(box try!(File::open(p))),
            None => None,
        })
    }
    
    fn new_ss<'a>(&mut self, ss_num: usize) -> Result<Box<Write+'a>> {
        let p = self.dir.join(PathBuf::from(format!("{}-ss{}.pip", self.basename, ss_num)));
        if self.ss.get(&ss_num).map_or(false, |&(ref p, _)| *p != PathBuf::new()) || p.exists() {
            return make_io_err(ErrorKind::AlreadyExists, "snapshot already exists");
        }
        let stream = try!(File::create(&p));
        if self.ss.contains_key(&ss_num) {
            self.ss.get_mut(&ss_num).unwrap().0 = p;
        } else {
            self.ss.insert(ss_num, (p, VecMap::new()));
        }
        Ok(box stream)
    }
    
    fn append_ss_cl<'a>(&mut self, ss_num: usize, cl_num: usize) -> Result<Box<Write+'a>> {
        match self.ss.get(&ss_num).and_then(|&(_, ref logs)| logs.get(&cl_num)) {
            Some(p) => {
                let stream = try!(OpenOptions::new().write(true).append(true).open(p));
                Ok(box stream)
            },
            None => make_io_err(ErrorKind::NotFound, "commit log file not found")
        }
    }
    fn new_ss_cl<'a>(&mut self, ss_num: usize, cl_num: usize) -> Result<Box<Write+'a>> {
        let mut logs = &mut self.ss.entry(ss_num).or_insert_with(|| (PathBuf::new(), VecMap::new())).1;
        if logs.contains_key(&cl_num) {
            return make_io_err(ErrorKind::AlreadyExists, "commit log file already exists");
        }
        let p = self.dir.join(PathBuf::from(format!("{}-ss{}-cl{}.pipl", self.basename, ss_num, cl_num)));
        let stream = try!(OpenOptions::new().create(true).write(true).append(true).open(&p));
        logs.insert(cl_num, p);
        Ok(box stream)
    }
}
