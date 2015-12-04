//! Pippin: file discovery

use std::path::{Path, PathBuf};
use std::io::{Read, Write, ErrorKind};
use std::fs::{read_dir, File, OpenOptions};
use std::cmp::max;
use regex::Regex;
use vec_map::VecMap;

use super::partition::PartitionIO;
use error::{Result, Error};


/// A helper to find files belonging to a partition (assuming a standard
/// layout on a local or mapped filesystem) and provide access.
/// 
/// As an alternative, users could provide their own implementations of
/// PartitionIO.
pub struct DiscoverPartitionFiles {
    dir: PathBuf,
    basename: String,  // first part of file name
    len: usize, // largest index in snapshots + 1
    snapshots: VecMap<PathBuf>,
    logs: VecMap<VecMap<PathBuf>>,
}

impl DiscoverPartitionFiles {
    /// Create a new instance.
    /// 
    /// `path` must be a directory containing (or in the case of a new repo, to
    /// contain) data files for the existing partition. `basename` is the first
    /// part of the file name, common to all files of this partition.
    pub fn from_dir_basename(path: &Path, basename: &str) -> Result<DiscoverPartitionFiles> {
        if !path.is_dir() { return Err(Error::path("not a directory", path.to_path_buf())); }
        //TODO: validate basename
        
        let ss_pat = try!(Regex::new("-ss([1-9][0-9]*).pip"));
        let cl_pat = try!(Regex::new("-ss([1-9][0-9]*)-cl([1-9][0-9]*).pipl"));
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
                if let Some(_replaced) = snapshots.insert(ss, entry.path()) {
                    panic!("multiple files map to same basname/number");
                }
            } else if let Some(caps) = cl_pat.captures(&fname[blen..]) {
                let ss: usize = try!(caps.at(1).expect("match should yield capture").parse());
                let cl: usize = try!(caps.at(2).expect("match should yield capture").parse());
                let s_vec = &mut logs.entry(ss).or_insert_with(|| VecMap::new());
                if let Some(_replaced) = s_vec.insert(cl, entry.path()) {
                    panic!("multiple files map to same basname/number");
                }
            } // else: no match; ignore
        }
        
        let len = max(snapshots.keys().next_back().unwrap_or(0),
                      logs.keys().next_back().unwrap_or(0));
        Ok(DiscoverPartitionFiles {
            dir: path.to_path_buf(),
            basename: basename.to_string(),
            len: len,
            snapshots: snapshots,
            logs: logs })
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
        let mut logs = VecMap::new();
        let mut dir_path = None;
        let mut basename = None;
        
        for path in paths.into_iter() {
            if !path.is_file() {
                return Err(Error::path("not a file", path));
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
                    if let Some(_replaced) = snapshots.insert(ss, path) {
                        panic!("multiple files map to same basname/number");
                    }
                },
                FileIs::CommitLog(ss, cl) => {
                    let s_vec = &mut logs.entry(ss).or_insert_with(|| VecMap::new());
                    if let Some(_replaced) = s_vec.insert(cl, path) {
                        panic!("multiple files map to same basname/number");
                    }
                },
                FileIs::BadFileName(msg) => {
                    return Err(Error::path(msg, path));
                },
            }
        }
        
        if basename == None {
            return Err(Error::io(ErrorKind::NotFound, "no path"));
        }
        // We can't use VecMap::len() because that's the number of elements. We
        // want the key of the last element plus one.
        let len = max(snapshots.keys().next_back().map(|x| x+1).unwrap_or(0),
                      logs.keys().next_back().map(|x| x+1).unwrap_or(0));
        Ok(DiscoverPartitionFiles {
            dir: dir_path.expect("dir_path should be set when basename is set"),
            basename: basename.unwrap(/*tested above*/),
            len: len,
            snapshots: snapshots,
            logs: logs })
    }
    
    fn getp_ss_cl(&self, ss_num: usize, cl_num: usize) -> Option<&PathBuf> {
        self.logs.get(&ss_num).and_then(|logs| logs.get(&cl_num))
    }
    
    /// Output the number of snapshot files found.
    pub fn num_ss_files(&self) -> usize {
        self.snapshots.len()
    }
    
    /// Output the number of log files found.
    pub fn num_cl_files(&self) -> usize {
        let mut num = 0;
        for ss_logs in self.logs.values() {
            num += ss_logs.len();
        }
        num
    }
    
    /// Returns a reference to the path of a snapshot file, if found.
    pub fn get_ss_path(&self, ss: usize) -> Option<&Path> {
        self.snapshots.get(&ss).map(|p| p.as_path())
    }
    
    /// Returns a reference to the path of a log file, if found.
    pub fn get_cl_path(&self, ss: usize, cl: usize) -> Option<&Path> {
        match self.logs.get(&ss) {
            Some(logs) => logs.get(&cl).map(|p| p.as_path()),
            None => None,
        }
    }
}

impl PartitionIO for DiscoverPartitionFiles {
    fn ss_len(&self) -> usize { self.len }
    fn ss_cl_len(&self, ss_num: usize) -> usize {
        match self.logs.get(&ss_num) {
            Some(logs) => logs.keys().next_back().map(|x| x+1).unwrap_or(0),
            None => 0,
        }
    }
    
    fn read_ss<'a>(&self, ss_num: usize) -> Result<Option<Box<Read+'a>>> {
        Ok(match self.snapshots.get(&ss_num) {
            Some(p) => Some(box try!(File::open(p))),
            None => None,
        })
    }
    
    fn read_ss_cl<'a>(&self, ss_num: usize, cl_num: usize) -> Result<Option<Box<Read+'a>>> {
        Ok(match self.getp_ss_cl(ss_num, cl_num) {
            Some(p) => Some(box try!(File::open(p))),
            None => None,
        })
    }
    
    fn new_ss<'a>(&mut self, ss_num: usize) -> Result<Box<Write+'a>> {
        if !self.snapshots.contains_key(&ss_num) {
            let p = self.dir.join(PathBuf::from(format!("{}-ss{}.pip", self.basename, self.len)));
            let stream = if !p.exists() {
                self.len = ss_num + 1;
                // TODO: atomic if-exists-don't-overwrite
                Some(try!(File::create(&p)))
            } else { None };
            self.snapshots.insert(ss_num, p);
            if let Some(s) = stream {
                return Ok(box s);
            }
        }
        Err(Error::io(ErrorKind::AlreadyExists, "snapshot file already exists"))
    }
    
    fn append_ss_cl<'a>(&mut self, ss_num: usize, cl_num: usize) -> Result<Box<Write+'a>> {
        match self.getp_ss_cl(ss_num, cl_num) {
            Some(p) => {
                let stream = try!(OpenOptions::new().write(true).append(true).open(p));
                Ok(box stream)
            },
            None => Err(Error::io(ErrorKind::NotFound, "commit log file not found"))
        }
    }
    fn new_ss_cl<'a>(&mut self, ss_num: usize, cl_num: usize) -> Result<Box<Write+'a>> {
        if self.getp_ss_cl(ss_num, cl_num) == None {
            let p = self.dir.join(PathBuf::from(format!("{}-ss{}-cl{}.pipl", self.basename, ss_num, cl_num)));
            let stream = if !p.exists() {
                Some(try!(OpenOptions::new().create(true).write(true).append(true).open(&p)))
            } else { None };
            self.logs.entry(ss_num).or_insert_with(|| VecMap::new()).insert(cl_num, p);
            if let Some(s) = stream {
                return Ok(box s);
            }
        }
        Err(Error::io(ErrorKind::AlreadyExists, "commit log file already exists"))
    }
}
