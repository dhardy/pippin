/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Pippin: file discovery

use std::path::{Path, PathBuf};
use std::io::{Read, Write, ErrorKind};
use std::fs::{read_dir, File, OpenOptions};
use std::any::Any;
use std::collections::hash_map::HashMap;
use std::collections::hash_map;

use regex::Regex;
use vec_map::{VecMap, Entry};
use walkdir::WalkDir;

use {PartIO, RepoIO, PartId};
use error::{Result, PathError, ArgError, make_io_err, OtherError};


/// A helper to find files belonging to a partition (assuming a standard
/// layout on a local or mapped filesystem) and provide access.
/// 
/// As an alternative, users could provide their own implementations of
/// PartIO.
#[derive(Debug)]
pub struct DiscoverPartFiles {
    part_id: Option<PartId>,
    dir: PathBuf,
    basename: String,  // first part of file name
    // Map of snapshot-number to pair (snapshot, map of log number to log)
    // The snapshot path may be empty (if not found).
    ss: VecMap<(PathBuf, VecMap<PathBuf>)>,
}

impl DiscoverPartFiles {
    /// Create a new instance.
    /// 
    /// `path` must be a directory containing (or in the case of a new repo, to
    /// contain) data files for the existing partition. `basename` is the first
    /// part of the file name, common to all files of this partition.
    /// 
    /// The `part_id` parameter lets the partition number be explicitly
    /// provided (useful if it is already known). If `None` is provided, then
    /// this function will attempt to discover the number from the file names.
    pub fn from_dir_basename(path: &Path, basename: &str, mut part_id: Option<PartId>) ->
            Result<DiscoverPartFiles>
    {
        if !path.is_dir() { return PathError::err("not a directory", path.to_path_buf()); }
        info!("Scanning for partition '{}...' files in: {}", basename, path.display());
        // Do basic validation of basename. As of now I am not sure exactly
        // which constraints it should conform to.
        if basename.contains('/') || basename.contains('\\') {
            return ArgError::err("basename must not contain any path separators");
        }
        
        let ss_pat = Regex::new("^-ss(0|[1-9][0-9]*).pip$").expect("valid regex");
        let cl_pat = Regex::new("^-ss(0|[1-9][0-9]*)-cl(0|[1-9][0-9]*).piplog$").expect("valid regex");
        let blen = basename.len();
        
        let mut snapshots = VecMap::new();
        
        for entry in try!(read_dir(path)) {
            let entry = try!(entry);
            let os_name = entry.file_name();    // must be named for lifetime
            let fname = match os_name.to_str() {
                Some(s) => s,
                None => { /* ignore non-unicode names */ continue; },
            };
            if fname[0..blen] != *basename {
                trace!("Ignoring file (does not match basename): {}", fname);
                continue;   // no match
            }
            let suffix = &fname[blen..];
            let used = if let Some(caps) = ss_pat.captures(suffix) {
                let ss: usize = try!(caps.at(1).expect("match should yield capture").parse());
                trace!("Adding snapshot {}: {}", ss, entry.path().display());
                match snapshots.entry(ss) {
                    Entry::Occupied(e) => {
                        let e: &mut (PathBuf, VecMap<PathBuf>) = e.into_mut();
                        assert!(e.0 == PathBuf::new(), "multiple files map to same basname/number");
                        e.0 = entry.path();
                    },
                    Entry::Vacant(e) => {
                        e.insert((entry.path(), VecMap::new()));
                    },
                };
                true
            } else if let Some(caps) = cl_pat.captures(suffix) {
                let ss: usize = try!(caps.at(1).expect("match should yield capture").parse());
                let cl: usize = try!(caps.at(2).expect("match should yield capture").parse());
                trace!("Adding snapshot {} log {}: {}", ss, cl, entry.path().display());
                let s_vec = &mut snapshots.entry(ss).or_insert_with(|| (PathBuf::new(), VecMap::new()));
                if let Some(_replaced) = s_vec.1.insert(cl, entry.path()) {
                    panic!("multiple files map to same basname/number");
                }
                true
            } else {
                trace!("Ignoring file (does not match regex): {}", fname);
                false
            };
            if used && part_id == None {
                // part_id not supplied: try to guess
                part_id = discover_part_num(fname);
            }
        }
        
        Ok(DiscoverPartFiles {
            part_id: part_id,
            dir: path.to_path_buf(),
            basename: basename.to_string(),
            ss: snapshots })
    }
    
    /// Create a new instance, loading only those paths given. Each path must
    /// be a Pippin file. 
    /// 
    /// Directory and base-name for files are taken from the first path given.
    /// 
    /// The `part_id` parameter lets the partition number be explicitly
    /// provided (useful if it is already known). If `None` is provided, then
    /// this function will attempt to discover the number from the file names.
    pub fn from_paths(paths: Vec<PathBuf>, mut part_id: Option<PartId>) ->
            Result<DiscoverPartFiles>
    {
        info!("Loading partition files...");
        // Note: there are no defined rules about which characters are allowed
        // in the basename, so just match anything.
        let ss_pat = try!(Regex::new(r"^(.+)-ss(0|[1-9][0-9]*).pip$"));
        let cl_pat = try!(Regex::new(r"^(.+)-ss(0|[1-9][0-9]*)-cl(0|[1-9][0-9]*).piplog$"));
        
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
            impl FileIs {
                fn pippin_file(&self) -> bool {
                    match self {
                        &FileIs::SnapShot(_) | &FileIs::CommitLog(_, _) => true,
                        &FileIs::BadFileName(_) => false,
                    }
                }
            }
            let file_is = {
                // Within this block we borrow from `path`, so the borrow checker will not us
                // move `path`. (A more precise checker might make allow this.)
                if let Some(fname) = path.file_name().expect("file path must have a file name").to_str() {
                    let file_is = if let Some(caps) = ss_pat.captures(fname) {
                        let fname_base = caps.at(1).expect("match should yield capture").to_string();
                        if basename == None {
                            basename = Some(fname_base);
                        } else if basename != Some(fname_base) {
                            // Different basename: probably different partition
                            // #0017: report warning or error?
                            continue;   // ignore this file
                        }
                        let ss: usize = try!(caps.at(2).expect("match should yield capture").parse());
                        FileIs::SnapShot(ss)
                    } else if let Some(caps) = cl_pat.captures(fname) {
                        let fname_base = caps.at(1).expect("match should yield capture").to_string();
                        if basename == None {
                            basename = Some(caps.at(1).expect("match should yield capture").to_string());
                        } else if basename != Some(fname_base) {
                            // Different basename: probably different partition
                            // #0017: report warning or error?
                            continue;   // ignore this file
                        }
                        let ss: usize = try!(caps.at(2).expect("match should yield capture").parse());
                        let cl: usize = try!(caps.at(3).expect("match should yield capture").parse());
                        FileIs::CommitLog(ss, cl)
                    } else {
                        if fname.ends_with(".pip") {
                            FileIs::BadFileName("Snapshot file names should have form BASENAME-ssNUM.pip")
                        } else if fname.ends_with(".piplog") {
                            FileIs::BadFileName("Commit log file names should have form BASENAME-ssNUM-clNUM.piplog")
                        } else {
                            FileIs::BadFileName("Not a Pippin file (name doesn't end .pip or .piplog")
                        }
                    };
                    if part_id == None && file_is.pippin_file() {
                        // part_id not supplied: try to guess
                        part_id = discover_part_num(fname);
                    }
                    file_is
                } else {
                    FileIs::BadFileName("could not convert file name to unicode")
                }
            };
            match file_is {
                // Decisions made. Now we can move path without worrying the borrow checker.
                FileIs::SnapShot(ss) => {
                    match snapshots.entry(ss) {
                        Entry::Vacant(e) => {
                            e.insert((path, VecMap::new()));
                        },
                        Entry::Occupied(mut e) => {
                            let value: &mut (PathBuf, VecMap<PathBuf>) = e.get_mut();
                            if value.0 != PathBuf::new() {
                                panic!("multiple files map to same basename/number");
                            }
                            value.0 = path;
                        },
                    };
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
        Ok(DiscoverPartFiles {
            part_id: part_id,
            dir: dir_path.expect("dir_path should be set when basename is set"),
            basename: basename.unwrap(/*tested above*/),
            ss: snapshots })
    }
    
    /// Explicitly set the partition identifier. This may be useful if wanting
    /// to try a number as a "fallback" after discovery fails.
    pub fn set_part_id(&mut self, part_id: PartId) {
        self.part_id = Some(part_id);
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

impl PartIO for DiscoverPartFiles {
    fn as_any(&self) -> &Any { self }
    
    fn part_id(&self) -> Option<PartId> { self.part_id }
    
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
            Some(&(ref p, _)) => {
                trace!("Reading snapshot file: {}", p.display());
                Some(box try!(File::open(p)))
            },
            None => None
        })
    }
    
    fn read_ss_cl<'a>(&self, ss_num: usize, cl_num: usize) -> Result<Option<Box<Read+'a>>> {
        Ok(match self.ss.get(&ss_num).and_then(|&(_, ref logs)| logs.get(&cl_num)) {
            Some(p) => {
                trace!("Reading log file: {}", p.display());
                Some(box try!(File::open(p)))
            },
            None => None,
        })
    }
    
    fn new_ss<'a>(&mut self, ss_num: usize) -> Result<Option<Box<Write+'a>>> {
        let p = self.dir.join(PathBuf::from(format!("{}-ss{}.pip", self.basename, ss_num)));
        if self.ss.get(&ss_num).map_or(false, |&(ref p, _)| *p != PathBuf::new()) || p.exists() {
            return Ok(None);
        }
        trace!("Creating snapshot file: {}", p.display());
        let stream = try!(File::create(&p));
        if self.ss.contains_key(&ss_num) {
            self.ss.get_mut(&ss_num).unwrap().0 = p;
        } else {
            self.ss.insert(ss_num, (p, VecMap::new()));
        }
        Ok(Some(box stream))
    }
    
    fn append_ss_cl<'a>(&mut self, ss_num: usize, cl_num: usize) -> Result<Option<Box<Write+'a>>> {
        Ok(match self.ss.get(&ss_num).and_then(|&(_, ref logs)| logs.get(&cl_num)) {
            Some(p) => {
                trace!("Appending to log file: {}", p.display());
                Some(box try!(OpenOptions::new().write(true).append(true).open(p)))
            },
            None => None
        })
    }
    fn new_ss_cl<'a>(&mut self, ss_num: usize, cl_num: usize) -> Result<Option<Box<Write+'a>>> {
        let mut logs = &mut self.ss.entry(ss_num).or_insert_with(|| (PathBuf::new(), VecMap::new())).1;
        if logs.contains_key(&cl_num) {
            return Ok(None);
        }
        let p = self.dir.join(PathBuf::from(format!("{}-ss{}-cl{}.piplog", self.basename, ss_num, cl_num)));
        trace!("Creating log file: {}", p.display());
        let stream = try!(OpenOptions::new().create(true).write(true).append(true).open(&p));
        logs.insert(cl_num, p);
        Ok(Some(box stream))
    }
}

/// A helper to discover a partition number from a file name (e.g.
/// `thing-pn15-ss12-cl0.piplog` has partition number 15).
pub fn discover_part_num(fname: &str) -> Option<PartId> {
    let ss_pat = Regex::new("^(.*)pn(0|[1-9][0-9]*)-ss(0|[1-9][0-9]*).pip$")
            .expect("valid regex");
    let cl_pat = Regex::new("^(.*)pn(0|[1-9][0-9]*)-ss(0|[1-9][0-9]*)-cl(0|[1-9][0-9]*).piplog$")
            .expect("valid regex");
    
    let caps = if let Some(caps) = ss_pat.captures(fname) {
        Some(caps)
    } else if let Some(caps) = cl_pat.captures(fname) {
        Some(caps)
    } else {
        None
    };
    caps.and_then(|caps| caps.at(2).expect("match should yield capture")
            .parse().ok().map(|n| PartId::from_num(n)))
}


/// A helper struct for finding repository files.
pub struct DiscoverRepoFiles {
    // top directory
    dir: PathBuf,
    // for each partition number, a path to the directory and a base-name
    partitions: HashMap<PartId, (PathBuf, String)>,
}
impl DiscoverRepoFiles {
    /// Discover all repository files in some directory (including recursively).
    pub fn from_dir(path: &Path) -> Result<DiscoverRepoFiles> {
        if !path.is_dir() { return PathError::err("not a directory", path.to_path_buf()); }
        info!("Scanning for repository files in: {}", path.display());
        
        let pat = Regex::new("^(.*)pn(0|[1-9][0-9]*)-ss(0|[1-9][0-9]*)\
                (.pip|-cl(0|[1-9][0-9]*).piplog)$").expect("valid regex");
        
        let mut paths = HashMap::new();
        
        for entry in WalkDir::new(path) {
            let entry = try!(entry);
            let fname = match entry.file_name().to_str() {
                Some(s) => s,
                None => { /* ignore non-unicode names */ continue; },
            };
            if let Some(caps) = pat.captures(fname) {
                let num: u64 = try!(caps.at(2).expect("match should yield capture").parse());
                let num = PartId::from_num(num);    //TODO: verify first for better error handling?
                // Ignore if we already have this partition number or have other error
                if !paths.contains_key(&num) {
                    let mut basename = caps.at(1).expect("match should yield capture").to_string();
                    basename.push_str("pn");
                    basename.push_str(caps.at(2).unwrap());
                    if let Some(dir) = entry.path().parent() {
                        trace!("Adding partition {}/{}...", dir.display(), basename);
                        paths.insert(num, (dir.to_path_buf(), basename));
                    }
                }
            }
        }
        
        Ok(DiscoverRepoFiles {
            dir: path.to_path_buf(),
            partitions: paths
        })
    }
    
    /// Discover partition files from a set of files
    pub fn from_paths(paths: Vec<PathBuf>) -> Result<DiscoverRepoFiles> {
        info!("Loading repository files...");
        
        let pat = Regex::new("^(.*)pn(0|[1-9][0-9]*)-ss(0|[1-9][0-9]*)\
                (.pip|-cl(0|[1-9][0-9]*).piplog)$").expect("valid regex");
        
        let mut top = None; // path to top directory
        let mut parts = HashMap::new();
        
        for path in paths {
            let fname = match path.file_name().and_then(|n| n.to_str()){
                Some(name) => name,
                None => {
                    // #0017: warn that path has no file name (is a dir?)
                    continue;
                },
            };
            if let Some(caps) = pat.captures(fname) {
                let num: u64 = try!(caps.at(2).expect("match should yield capture").parse());
                let num = PartId::from_num(num);    //TODO: verify first for better error handling?
                let mut basename = caps.at(1).expect("match should yield capture").to_string();
                basename.push_str("pn");
                basename.push_str(caps.at(2).unwrap());
                let dir = path.parent().expect("path has parent").to_path_buf();
                if top == None {
                    top = Some(dir.clone());
                } else if top.as_ref() != Some(&dir) {
                    // Since directories differ, creating new partitions may
                    // put files in the wrong place. We could refuse to create
                    // new partitions, but that's not exactly helpful. We could
                    // warn about it, but that doesn't seem especially useful.
                }
                match parts.entry(num) {
                    hash_map::Entry::Vacant(e) => { e.insert((dir, basename)); },
                    hash_map::Entry::Occupied(_) => {
                        // #0017: warn if dir/basename differ
                    }
                };
            }
        }
        
        if let Some(path) = top {
            Ok(DiscoverRepoFiles {
                dir: path,
                partitions: parts
            })
        }else {
            return OtherError::err("no Pippin files found!");
        }
    }
    
    /// Iterate over partitions
    pub fn partitions(&self) -> RepoPartIter {
        RepoPartIter { iter: self.partitions.iter() }
    }
}
impl RepoIO for DiscoverRepoFiles {
    fn as_any(&self) -> &Any { self }
    fn num_partitions(&self) -> usize {
        self.partitions.len()
    }
    fn partitions(&self) -> Vec<PartId> {
        self.partitions.keys().map(|n| *n).collect()
    }
    fn add_partition(&mut self, num: PartId, prefix: &str) -> Result<()> {
        let mut path = self.dir.clone();
        let mut prefix = prefix;
        while let Some(pos) = prefix.find('/') {
            path.push(Path::new(&prefix[..pos]));
            prefix = &prefix[pos+1..];
        }
        let basename = format!("{}pn{}", prefix, num);
        self.partitions.insert(num, (path, basename));
        Ok(())
    }
    fn make_partition_io(&self, num: PartId) -> Result<Box<PartIO>> {
        if let Some(&(ref path, ref basename)) = self.partitions.get(&num) {
            Ok(box try!(DiscoverPartFiles::from_dir_basename(path, basename, Some(num))))
        } else {
            make_io_err(ErrorKind::NotFound, "partition not found")
        }
    }
}

/// Item representing a partition discovered by a `DiscoverPartFiles`.
pub struct RepoPartItem<'a> {
    part_id: PartId,
    data: &'a (PathBuf, String),
}
impl<'a> RepoPartItem<'a> {
    /// Get the partition's identifier
    pub fn part_id(&self) -> PartId {
        self.part_id
    }
    /// Get the partition's directory
    pub fn dir(&self) -> &'a Path {
        &self.data.0
    }
    /// Get the partition's base-name
    /// 
    /// (This is an implementation detail and not guaranteed to remain fixed.)
    pub fn basename(&self) -> &'a str {
        &self.data.1
    }
}
/// Iterator over partitions discovered by a `DiscoverRepoFiles`.
pub struct RepoPartIter<'a> {
    iter: hash_map::Iter<'a, PartId, (PathBuf, String)>
}
impl<'a> Iterator for RepoPartIter<'a> {
    type Item = RepoPartItem<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|item|
            RepoPartItem {
                part_id: *item.0,
                data: item.1,
            }
        )
    }
    fn size_hint(&self) -> (usize, Option<usize>) { self.iter.size_hint() }
}
