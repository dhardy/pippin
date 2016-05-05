/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Pippin: file discovery

use std::path::{Path, PathBuf};
use std::io::{ErrorKind};
use std::fs::{read_dir, File};
use std::any::Any;
use std::collections::hash_map::HashMap;
use std::collections::hash_map;

use regex::Regex;
use vec_map::{VecMap, Entry};
use walkdir::WalkDir;

use {PartIO, RepoIO, PartId};
use fileio::PartFileIO;
use detail::readwrite::read_head;
use error::{Result, PathError, make_io_err, OtherError};


/// Will attempt to discover partition files from a path.
/// 
/// The `path` argument is used to discover files. If it points to a directory,
/// all `.pip` and `.piplog` files in this directory (non-recursively) are
/// found. If it points to a `.pip` or `.piplog` file, then this file and other
/// files in the same directory with the same basename are found.
/// 
/// TODO: consider supporting blobs or partial file names (i.e. patterns of
/// some kind). Is there any use-case besides lazy entry in command-line tools?
/// 
/// If `opt_part_num` is `None`, this will discover the partition number from
/// the files found, and fail if not all files found correspond to the same
/// partition. If `opt_part_num` is some `PartId`, then when files are only
/// found for one partition, the partition number is assumed without
/// confirmation; when files from multiple partitions are found, they are
/// filtered. Either way this fails if no files are found for the right
/// partition.
pub fn part_from_path(path: &Path, opt_part_num: Option<PartId>) -> Result<PartFileIO> {
    let ss_pat = Regex::new("^(.*)-ss(0|[1-9][0-9]*)\\.pip$").expect("valid regex");
    let cl_pat = Regex::new("^(.*)-ss(0|[1-9][0-9]*)-cl(0|[1-9][0-9]*)\\.piplog$").expect("valid regex");
    
    let mut part_id: Option<PartId> = opt_part_num;
    let mut basename: Option<String> = None;
    
    let dir = if path.is_dir() {
        info!("Scanning for partition files in: {}", path.display());
        path
    } else if let Some(fname) = path.file_name() {
        let fname = try!(fname.to_str().ok_or(PathError::new("not valid UTF-8", path.to_path_buf())));
        if let Some(bname) = discover_basename(fname) {
            let dir = try!(path.parent().ok_or(PathError::new("path has no parent", path.to_path_buf())));
            info!("Scanning for partition files matching: {}/{}*", dir.display(), bname);
            part_id = Some(try!(find_part_num(&bname, path)));
            basename = Some(bname);
            dir
        } else {
            return PathError::err("discover::part_from_path: not a Pippin file",
                    path.to_path_buf());
        }
    } else {
        return PathError::err("discover::part_from_path: neither a file nor a directory",
                path.to_path_buf())
    };
    
    let mut snapshots = VecMap::new();
    
    {   // new scope for filter_skip closure
    let mut filter_skip = |bname: &str, path: &Path| -> Result<bool> {
        if let &Some(ref req_bname) = &basename {
            // basename known: filter by it
            if bname == req_bname {
                // match; good
            } else if let Some(req_pnum) = opt_part_num {
                if try!(find_part_num(bname, path)) == req_pnum {
                    // match but different basename; okay but warn
                    warn!("Multiple file prefixes for same same partition number! ({}, {})",
                        req_bname, bname);
                } else {
                    // different partition number: skip
                    return Ok(true);
                }
            } else {
                // Not filtering by partition number: skip because basename differs
                return Ok(true);
            }
        } else if let Some(req_pnum) = opt_part_num {
            // basename not known but  part_num is: filter by num
            if try!(find_part_num(bname, path)) != req_pnum {
                // wrong partition number: skip
                return Ok(true);
            }
        }
        // done filtering; update basename etc. if necessary
        if basename == None {
            if part_id == None {
                // Set part_id if not supplied.
                part_id = Some(try!(find_part_num(bname, path)));
            }
            basename = Some(bname.to_string()); // assume
        }
        Ok(false)   /* do not skip */
    };
    
    for entry in try!(read_dir(dir)) {
        // —— Get file name ——
        let entry = try!(entry);
        let fpath = &entry.path();
        let os_name = entry.file_name();    // must be named for lifetime
        let fname = match os_name.to_str() {
            Some(s) => s,
            None => {
                warn!("Non-Unicode filename; ignoring: {}", fpath.display());
                continue;
            },
        };
        
        // —— Match, filter and add ——
        if let Some(caps) = ss_pat.captures(fname) {
            let bname = caps.at(1).expect("match has capture");
            if try!(filter_skip(bname, fpath)) { continue; }
            
            let ss: usize = try!(caps.at(2).expect("match has capture").parse());
            trace!("Adding snapshot {}: {}", ss, fpath.display());
            match snapshots.entry(ss) {
                Entry::Occupied(e) => {
                    let e: &mut (Option<PathBuf>, VecMap<PathBuf>) = e.into_mut();
                    assert!(e.0 == None, "multiple files map to same basename/number");
                    e.0 = Some(entry.path());
                },
                Entry::Vacant(e) => {
                    e.insert((Some(entry.path()), VecMap::new()));
                },
            };
        } else if let Some(caps) = cl_pat.captures(fname) {
            let bname = caps.at(1).expect("match has capture");
            if try!(filter_skip(bname, fpath)) { continue; }
            
            let ss: usize = try!(caps.at(2).expect("match has capture").parse());
            let cl: usize = try!(caps.at(3).expect("match has capture").parse());
            trace!("Adding snapshot {} log {}: {}", ss, cl, fpath.display());
            let s_vec = &mut snapshots.entry(ss).or_insert_with(|| (None, VecMap::new()));
            if let Some(_replaced) = s_vec.1.insert(cl, entry.path()) {
                // this should be impossible, hence panic:
                panic!("multiple files map to same basename/number");
            }
        } else {
            trace!("Ignoring file (does not match regex): {}", fname);
        }
    }
    }   // destroy filter_skip: end borrow on part_id and basename
    
    if let Some(bname) = basename {
        let part_id = part_id.expect("has part_id when has basename");
        Ok(PartFileIO::new(part_id, dir.join(bname), snapshots))
    } else {
        Err(Box::new(if opt_part_num.is_some() {
            PathError::new("discover::part_from_path: no files found matching part num in", path.to_path_buf())
        } else {
            // Input path is either a dir or a file; when a file part_id is
            // found (or fn aborts earlier), hence path is a dir
            PathError::new("discover::part_from_path: no Pippin files found in dir", path.to_path_buf())
        }))
    }
}


/// A helper to discover a partition number from a file name or header (e.g.
/// `thing-pn15-ss12-cl0.piplog` has partition number 15, `pn12` has number
/// 12). A prefix before `pn` is allowed. The `pn[0-9]+` part must either end
/// the given string or be followed by a full Pippin filename pattern.
/// 
/// `name`: full filename or basename
pub fn part_num_from_name(name: &str) -> Option<PartId> {
    let pat = Regex::new("^.*pn(0|[1-9][0-9]*)(-ss(0|[1-9][0-9]*)(\\.pip|-cl(0|[1-9][0-9]*)\\.piplog))?$")
            .expect("valid regex");
    
    pat.captures(name)
            .and_then(|caps| caps.at(1).expect("match has capture")
            .parse().ok().map(|n| PartId::from_num(n)))
}   
/// A wrapper around `part_num_from_name` which discovers the number from the
/// name if possible and otherwise reads the header to find the partition
/// number. Fails if it can't find one.
/// 
/// `name`: full filename or basename
/// `path`: full path to file
pub fn find_part_num(name: &str, path: &Path) -> Result<PartId> {
    if let Some(num) = part_num_from_name(name) {
        return Ok(num);
    }
    let head = try!(read_head(&mut try!(File::open(path))));
    head.part_id.ok_or(box OtherError::new("file contains no part id"))
}
/// A helper to try matching a file name against standard Pippin file patterns,
/// and if it fits return the "basename" part.
pub fn discover_basename(fname: &str) -> Option<String> {
    let pat = Regex::new("^(.*)-ss(0|[1-9][0-9]*)(\\.pip|-cl(0|[1-9][0-9]*)\\.piplog)$")
            .expect("valid regex");
    
    pat.captures(fname)
            .map(|caps| caps.at(1).expect("match has capture").to_string())
}


// —————  Repository  —————

/// Remembers a set of partitions, each by its partition number and prefix
/// (directory relative to repository root and file "basename"). Can create a
/// PartFileIO for each. Can create new partitions.
/// 
/// This is in the `discover` module because it relies on discovering partition
/// files from a name and number (`part_from_path`) every time
/// `make_partition_io` is called. The advantage is that the file list will be
/// up-to-date every time a partition is opened.
pub struct RepoFileIO {
    // Top directory of partition (which paths are relative to)
    dir: PathBuf,
    // For each partition number, a prefix
    partitions: HashMap<PartId, PathBuf>,
}
impl RepoFileIO {
    //TODO: rethink from_dir and from_paths. They should work when "pn" isn't
    // part of the name. Should we just use part_from_path and create the
    // PartIO objects now, then clone?
    
    /// Discover all repository files in some directory (including recursively).
    pub fn from_dir(path: &Path) -> Result<RepoFileIO> {
        if !path.is_dir() { return PathError::err("not a directory", path.to_path_buf()); }
        info!("Scanning for repository files in: {}", path.display());
        
        let pat = Regex::new("^(.*)pn(0|[1-9][0-9]*)-ss(0|[1-9][0-9]*)\
                (\\.pip|-cl(0|[1-9][0-9]*)\\.piplog)$").expect("valid regex");
        
        let mut paths = HashMap::new();
        
        for entry in WalkDir::new(path) {
            let entry = try!(entry);
            let fname = match entry.file_name().to_str() {
                Some(s) => s,
                None => { /* ignore non-unicode names */ continue; },
            };
            if let Some(caps) = pat.captures(fname) {
                let num: u64 = try!(caps.at(2).expect("match has capture").parse());
                let num = PartId::from_num(num);
                // Ignore if we already have this partition number or have other error
                if !paths.contains_key(&num) {
                    let mut basename = caps.at(1).expect("match has capture").to_string();
                    basename.push_str("pn");
                    basename.push_str(caps.at(2).unwrap());
                    if let Some(dir) = entry.path().parent() {
                        trace!("Adding partition {}/{}...", dir.display(), basename);
                        paths.insert(num, dir.join(basename));
                    }
                }
            }
        }
        
        Ok(RepoFileIO {
            dir: path.to_path_buf(),
            partitions: paths
        })
    }
    
    /// Discover partition files from a set of files
    pub fn from_paths(paths: Vec<PathBuf>) -> Result<RepoFileIO> {
        info!("Loading repository files...");
        
        let pat = Regex::new("^(.*)pn(0|[1-9][0-9]*)-ss(0|[1-9][0-9]*)\
                (.pip|-cl(0|[1-9][0-9]*)\\.piplog)$").expect("valid regex");
        
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
                let num: u64 = try!(caps.at(2).expect("match has capture").parse());
                let num = PartId::from_num(num);
                let mut basename = caps.at(1).expect("match has capture").to_string();
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
                    hash_map::Entry::Vacant(e) => { e.insert(dir.join(basename)); },
                    hash_map::Entry::Occupied(_) => {
                        // #0017: warn if dir/basename differ
                    }
                };
            }
        }
        
        if let Some(path) = top {
            Ok(RepoFileIO {
                dir: path,
                partitions: parts
            })
        }else {
            return OtherError::err("no Pippin files found!");
        }
    }
}

impl RepoFileIO {
    /// Iterate over partitions
    pub fn partitions(&self) -> RepoPartIter {
        RepoPartIter { iter: self.partitions.iter() }
    }
}
impl RepoIO for RepoFileIO {
    fn as_any(&self) -> &Any { self }
    fn num_partitions(&self) -> usize {
        self.partitions.len()
    }
    fn partitions(&self) -> Vec<PartId> {
        self.partitions.keys().map(|n| *n).collect()
    }
    fn add_partition(&mut self, num: PartId, prefix: &str) -> Result<()> {
        // "pn{}" part is not essential so long as prefix is unique but is useful
        let path = self.dir.join(format!("{}pn{}", prefix, num));
        self.partitions.insert(num, path);
        Ok(())
    }
    fn make_partition_io(&self, num: PartId) -> Result<Box<PartIO>> {
        if let Some(ref path) = self.partitions.get(&num) {
            // TODO: it might be useful to pass the basename here since we have it
            // We pass the 'parent' since part_from_path can't currently take pattern arguments
            // FIXME: this doesn't work for new partitions
            Ok(box try!(part_from_path(path.parent().expect("has parent"), Some(num))))
        } else {
            make_io_err(ErrorKind::NotFound, "partition not found")
        }
    }
}

/// Iterator over the partitions in a `RepoFileIO`.
pub struct RepoPartIter<'a> {
    iter: hash_map::Iter<'a, PartId, PathBuf>
}
impl<'a> Iterator for RepoPartIter<'a> {
    type Item = RepoPartItem<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|result| RepoPartItem::from(result))
    }
    fn size_hint(&self) -> (usize, Option<usize>) { self.iter.size_hint() }
}
/// Item type of iterator
pub struct RepoPartItem<'a> {
    part_id: PartId,
    prefix: &'a Path,
}
impl<'a> RepoPartItem<'a> {
    /// Get the partition number
    pub fn part_id(&self) -> PartId { self.part_id }
    /// Get the path prefix for partition files (e.g.
    /// `/path/to/repo/this_part/this-part-pn5`). This may be an absolute path
    /// prefix or may be relative to the working directory.
    pub fn path_prefix(&self) -> &'a Path { self.prefix }
}
impl<'a> From<(&'a PartId, &'a PathBuf)> for RepoPartItem<'a> {
    fn from(v: (&'a PartId, &'a PathBuf)) -> Self {
        RepoPartItem { part_id: *v.0, prefix: v.1 }
    }
}
