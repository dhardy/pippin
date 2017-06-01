/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Pippin: file access for repositories and partitions.

use std::path::{Path, PathBuf};
use std::io::{Read, Write};
use std::fs::{File, OpenOptions};
use std::any::Any;
use std::ops::Add;
use std::collections::hash_map::{self, HashMap};

use vec_map::{VecMap, Entry};

use io::{PartIO, PartId, RepoIO};
use error::{Result, ReadOnly, OtherError};


// —————  Partition  —————

/// Data structure used in a `PartFileIO` to actually store file paths.
#[derive(Debug, Clone)]
pub struct PartPaths {
    // First key is snapshot number. Value is (if found) a path to the snapshot
    // file and a map of log paths.
    // Key of internal map is log number. Value is a path to the log file.
    paths: VecMap<(Option<PathBuf>, VecMap<PathBuf>)>
}
impl PartPaths {
    /// Create an empty structure.
    pub fn new() -> PartPaths { PartPaths { paths: VecMap::new() } }
    
    fn ss_len(&self) -> usize {
        self.paths.keys().next_back().map(|x| x+1).unwrap_or(0)
    }
    fn ss_cl_len(&self, ss_num: usize) -> usize {
        self.paths.get(ss_num) // Option<(_, VecMap<PathBuf>)>
            .and_then(|&(_, ref logs)| logs.keys().next_back())
            .map(|x| x+1).unwrap_or(0)
    }
    
    /// Count the snapshot files present.
    pub fn num_ss_files(&self) -> usize {
        self.paths.values().filter(|v| v.0.is_some()).count()
    }
    /// Count the log files present.
    pub fn num_cl_files(&self) -> usize {
        // #0018: could use `.sum()` but see https://github.com/rust-lang/rust/issues/27739
        self.paths.values().map(|v| v.1.len()).fold(0, Add::add)
    }
    
    /// Returns a reference to the path of a snapshot file path, if found.
    pub fn get_ss(&self, ss: usize) -> Option<&Path> {
        self.paths.get(ss).and_then(|&(ref p, _)| p.as_ref().map(|path| path.as_path()))
    }
    /// Returns a reference to the path of a log file, if found.
    pub fn get_cl(&self, ss: usize, cl: usize) -> Option<&Path> {
        self.paths.get(ss)     // Option<(_, VecMap<PathBuf>)>
            .and_then(|&(_, ref logs)| logs.get(cl))    // Option<PathBuf>
            .map(|p| p.as_path())
    }
    
    /// Add a path to the list of known files. This does not do any checking.
    /// 
    /// If a file with this snapshot number was previously known, it is replaced
    /// and `true` returned; otherwise `false` is returned.
    pub fn insert_ss(&mut self, ss_num: usize, path: PathBuf) -> bool {
        match self.paths.entry(ss_num) {
            Entry::Occupied(e) => {
                let has_previous = e.get().0.is_some();
                e.into_mut().0 = Some(path);
                has_previous
            }
            Entry::Vacant(e) => {
                e.insert((Some(path), VecMap::new()));
                false
            },
        }
    }
    /// Add a path to the list of known files. This does not do any checking.
    /// 
    /// If a file with this snapshot number and commit-log number was
    /// previously known, it is replaced and `true` returned; otherwise `false`
    /// is returned.
    pub fn insert_cl(&mut self, ss_num: usize, cl_num: usize, path: PathBuf) -> bool {
        self.paths.entry(ss_num)
                .or_insert_with(|| (None, VecMap::new()))
                .1.insert(cl_num, path) /* returns old value */
                .is_some() /* i.e. something was replaced */
    }
}

/// Remembers a set of file names associated with a partition, opens read
/// and write streams on these and creates new partition files.
#[derive(Debug, Clone)]
pub struct PartFileIO {
    readonly: bool,
    // Appended with snapshot/log number and extension to get a file path
    prefix: PathBuf,
    paths: PartPaths,
}

impl PartFileIO {
    /// Create an empty partition IO with partition identifier 1.
    /// 
    /// *   `prefix` is a dir + partial-file-name; it is appended with
    ///     something like `-ss1.pip` or `-ss2-cl3.piplog` to get a file name
    pub fn new_default<P: Into<PathBuf>>(prefix: P) -> PartFileIO {
        Self::new(prefix, PartPaths::new())
    }
    
    /// Create an empty partition IO. This is equivalent to calling `new` with
    /// `VecMap::new()` as the third argument.
    /// 
    /// *   `prefix` is a dir + partial-file-name; it is appended with
    ///     something like `-ss1.pip` or `-ss2-lf3.piplog` to get a file name
    pub fn new_empty<P: Into<PathBuf>>(prefix: P) -> PartFileIO {
        Self::new(prefix, PartPaths::new())
    }
    
    /// Create a partition IO with paths to some existing files.
    /// 
    /// *   `prefix` is a dir + partial-file-name; it is appended with
    ///     something like `-ss1.pip` or `-ss2-lf3.piplog` to get a file name
    /// *   `paths` is a list of paths of all known partition files
    pub fn new<P: Into<PathBuf>>(prefix: P, paths: PartPaths) -> PartFileIO
    {
        let prefix = prefix.into();
        trace!("New PartFileIO; prefix: {}, ss_len: {}", prefix.display(), paths.ss_len());
        PartFileIO {
            readonly: false,
            prefix: prefix,
            paths: paths,
        }
    }
    
    /// Get property: is this readonly? If this is readonly, file creation and
    /// modification of `RepoFileIO` and `PartFileIO` operations will be
    /// inhibited (operations will return a `ReadOnly` error).
    pub fn readonly(&self) -> bool {
        self.readonly
    }
    
    /// Set readonly. If this is readonly, file creation and
    /// modification of `RepoFileIO` and `PartFileIO` operations will be
    /// inhibited (operations will return a `ReadOnly` error).
    pub fn set_readonly(&mut self, readonly: bool) {
        self.readonly = readonly;
    }
    
    /// Get a reference to the prefix
    pub fn prefix(&self) -> &Path {
        &self.prefix
    }
    /// Get a reference to the internal store of paths
    pub fn paths(&self) -> &PartPaths {
        &self.paths
    }
    /// Get a mutable reference to the internal store of paths
    pub fn mut_paths(&mut self) -> &mut PartPaths {
        &mut self.paths
    }
}

impl PartIO for PartFileIO {
    fn as_any(&self) -> &Any { self }
    
    fn ss_len(&self) -> usize {
        self.paths.ss_len()
    }
    fn ss_cl_len(&self, ss_num: usize) -> usize {
        self.paths.ss_cl_len(ss_num)
    }
    
    fn has_ss(&self, ss_num: usize) -> bool {
        self.paths.paths.get(ss_num).map(|&(ref p, _)| p.is_some()).unwrap_or(false)
    }
    
    fn read_ss<'a>(&'a self, ss_num: usize) -> Result<Option<Box<Read+'a>>> {
        // Cannot replace `match` with `map` since `try!()` cannot be used in a closure
        Ok(match self.paths.paths.get(ss_num) {
            Some(&(ref p, _)) => {
                if let Some(ref path) = *p {
                    trace!("Reading snapshot file: {}", path.display());
                    Some(Box::new(File::open(path)?))
                } else {
                    None
                }
            },
            None => None
        })
    }
    
    fn read_ss_cl<'a>(&'a self, ss_num: usize, cl_num: usize) -> Result<Option<Box<Read+'a>>> {
        Ok(match self.paths.paths.get(ss_num).and_then(|&(_, ref logs)| logs.get(cl_num)) {
            Some(p) => {
                trace!("Reading log file: {}", p.display());
                Some(Box::new(File::open(p)?))
            },
            None => None,
        })
    }
    
    fn new_ss<'a>(&'a mut self, ss_num: usize) -> Result<Option<Box<Write+'a>>> {
        if self.readonly {
            return ReadOnly::err();
        }
        let mut p = self.prefix.as_os_str().to_os_string();
        p.push(format!("-ss{}.pip", ss_num));
        let p = PathBuf::from(p);
        if self.paths.paths.get(ss_num).map_or(false, |&(ref p, _)| p.is_some()) || p.exists() {
            // File already exists in internal map or on filesystem
            return Ok(None);
        }
        trace!("Creating snapshot file: {}", p.display());
        let stream = File::create(&p)?;
        match self.paths.paths.entry(ss_num) {
            Entry::Occupied(mut entry) => { entry.get_mut().0 = Some(p); },
            Entry::Vacant(entry) => { entry.insert((Some(p), VecMap::new())); },
        };
        Ok(Some(Box::new(stream)))
    }
    
    fn append_ss_cl<'a>(&'a mut self, ss_num: usize, cl_num: usize) -> Result<Option<Box<Write+'a>>> {
        if self.readonly {
            return ReadOnly::err();
        }
        Ok(match self.paths.paths.get(ss_num).and_then(|&(_, ref logs)| logs.get(cl_num)) {
            Some(p) => {
                trace!("Appending to log file: {}", p.display());
                Some(Box::new(OpenOptions::new().write(true).append(true).open(p)?))
            },
            None => None
        })
    }
    fn new_ss_cl<'a>(&'a mut self, ss_num: usize, cl_num: usize) -> Result<Option<Box<Write+'a>>> {
        if self.readonly {
            return ReadOnly::err();
        }
        let mut logs = &mut self.paths.paths.entry(ss_num).or_insert_with(|| (None, VecMap::new())).1;
        let mut p = self.prefix.as_os_str().to_os_string();
        p.push(format!("-ss{}-cl{}.piplog", ss_num, cl_num));
        let p = PathBuf::from(p);
        if logs.contains_key(cl_num) || p.exists() {
            // File already exists in internal map or on filesystem
            return Ok(None);
        }
        trace!("Creating log file: {}", p.display());
        let stream = OpenOptions::new().create(true).write(true).append(true).open(&p)?;
        logs.insert(cl_num, p);
        Ok(Some(Box::new(stream)))
    }
}


// —————  Repository  —————

/// Stores a set of `PartFileIO`s, each of which stores the paths of its files.
/// This is not "live" and could get out-of-date if another process touches the
/// files or if multiple `PartIO`s are requested for the same partition in this
/// process.
pub struct RepoFileIO {
    readonly: bool,
    // Top directory of partition (which paths are relative to)
    dir: PathBuf,
    // PartFileIO for each partition.
    parts: HashMap<PartId, PartFileIO>,
}
impl RepoFileIO {
    /// Create a new instance. This could be for a new repository or existing
    /// partitions can be added afterwards with `insert_part(prefix, part)`.
    /// 
    /// *   `dir` is the top directory, in which all data files are (as a
    ///     `String`, `Path` or anything which converts to a `PathBuf`)
    pub fn new<P: Into<PathBuf>>(dir: P) -> RepoFileIO {
        let dir = dir.into();
        trace!("New RepoFileIO; dir: {}", dir.display());
        RepoFileIO { readonly: false, dir: dir, parts: HashMap::new() }
    }
    
    /// Get property: is this readonly? If this is readonly, file creation and
    /// modification of `RepoFileIO` and `PartFileIO` operations will be
    /// inhibited (operations will return a `ReadOnly` error).
    pub fn readonly(&self) -> bool {
        self.readonly
    }
    
    /// Set readonly. If this is readonly, file creation and
    /// modification of `RepoFileIO` and `PartFileIO` operations will be
    /// inhibited (operations will return a `ReadOnly` error).
    pub fn set_readonly(&mut self, readonly: bool) {
        if readonly != self.readonly {
            for part in self.parts.values_mut() {
                part.set_readonly(readonly);
            }
            self.readonly = readonly;
        }
    }
    
    /// Add a (probably existing) partition to the repository. This differs
    /// from `RepoIO::add_partition` in that the prefix is specified in full
    /// here and a `PartFileIO` is passed, where `add_partition` creates a new
    /// one.
    /// 
    /// Returns true if there was not already a partition with this number
    /// present, or false if a partition with this number just got replaced.
    pub fn insert_part(&mut self, part_id: PartId, mut part: PartFileIO) -> bool {
        part.set_readonly(self.readonly);
        self.parts.insert(part_id, part).is_none()
    }
    /// Iterate over partitions
    pub fn partitions(&self) -> RepoPartIter {
        RepoPartIter { iter: self.parts.iter() }
    }
}
impl RepoIO for RepoFileIO {
    fn as_any(&self) -> &Any { self }
    fn num_parts(&self) -> usize {
        self.parts.len()
    }
    fn parts(&self) -> Vec<PartId> {
        self.parts.keys().cloned().collect()
    }
    fn has_part(&self, pn: PartId) -> bool {
        self.parts.contains_key(&pn)
    }
    fn new_part(&mut self, num: PartId, prefix: String) -> Result<()> {
        if self.readonly {
            return ReadOnly::err();
        }
        let path = self.dir.join(prefix);
        self.parts.insert(num, PartFileIO::new_empty(path));
        Ok(())
    }
    fn make_part_io(&mut self, num: PartId) -> Result<Box<PartIO>> {
        if let Some(io) = self.parts.get(&num) {
            Ok(Box::new((*io).clone()))
        } else {
            OtherError::err("partition not found")
        }
    }
}

/// Iterator over the partitions in a `RepoFileIO`.
pub struct RepoPartIter<'a> {
    iter: hash_map::Iter<'a, PartId, PartFileIO>
}
impl<'a> Iterator for RepoPartIter<'a> {
    type Item = (&'a PartId, &'a PartFileIO);
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
    fn size_hint(&self) -> (usize, Option<usize>) { self.iter.size_hint() }
}
