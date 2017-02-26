/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Pippin: file discovery

use std::path::Path;
use std::fs::{read_dir, File};
use std::collections::hash_map::{HashMap, Entry};

use regex::Regex;
use walkdir::WalkDir;

use {RepoIO, PartId};
use fileio::{PartFileIO, RepoFileIO, PartPaths};
use readwrite::read_head;
use error::{Result, PathError, OtherError};


/// Will attempt to discover files belonging to a single partition from a path.
/// 
/// The `path` argument is used to discover files. If it points to a directory,
/// the method scans for all `.pip` and `.piplog` files in this directory
/// (non-recursively).
/// If it points to a `.pip` or `.piplog` file, then the method will look for
/// all files in the same directory and with the same prefix (the part before
/// the snapshot number, `ssN`).
/// 
/// #0040: consider supporting blobs or partial file names (i.e. patterns of
/// some kind). Is there any use-case besides lazy entry in command-line tools?
/// 
/// If `opt_part_num` is `None`, this will discover the partition number from
/// the files found, and fail if not all files found correspond to the same
/// partition. If `opt_part_num` is some `PartId`, then when files are only
/// found for one partition, the partition number is assumed without
/// confirmation; when files from multiple partitions are found, they are
/// filtered. Either way this fails if no files are found for the right
/// partition.
pub fn part_from_path<P: AsRef<Path>>(path: P, opt_part_num: Option<PartId>) -> Result<(PartId, PartFileIO)> {
    let path = path.as_ref();
    let ss_pat = Regex::new("^((?:.*)-)?ss(0|[1-9][0-9]*)\\.pip$").expect("valid regex");
    let cl_pat = Regex::new("^((?:.*)-)?ss(0|[1-9][0-9]*)-cl(0|[1-9][0-9]*)\\.piplog$").expect("valid regex");
    
    let mut part_id: Option<PartId> = opt_part_num;
    let mut basename: Option<String> = None;
    
    let dir = if path.is_dir() {
        info!("Scanning for partition files in: {}", path.display());
        path
    } else if let Some(fname) = path.file_name() {
        let fname = fname.to_str().ok_or(PathError::new("not valid UTF-8", path))?;
        if let Some(bname) = discover_basename(fname) {
            let dir = path.parent().ok_or(PathError::new("path has no parent", path))?;
            info!("Scanning for partition files matching: {}/{}*", dir.display(), bname);
            part_id = Some(find_part_num(&bname, path)?);
            basename = Some(bname);
            dir
        } else {
            return PathError::err("discover::part_from_path: not a Pippin file", path);
        }
    } else {
        return PathError::err("discover::part_from_path: neither a file nor a directory", path)
    };
    
    let mut part_paths = PartPaths::new();
    
    {   // new scope for filter_skip closure
    let mut filter_skip = |bname: &str, path: &Path| -> Result<bool> {
        if let &Some(ref req_bname) = &basename {
            // basename known: filter by it
            if bname == req_bname {
                // match; good
            } else if let Some(req_pnum) = opt_part_num {
                if find_part_num(bname, path)? == req_pnum {
                    // match but different basename; okay but warn
                    warn!("Multiple file prefixes for same partition number! ({}, {})",
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
            if find_part_num(bname, path)? != req_pnum {
                // wrong partition number: skip
                return Ok(true);
            }
        }
        // done filtering; update basename etc. if necessary
        if basename == None {
            if part_id == None {
                // Set part_id if not supplied.
                part_id = Some(find_part_num(bname, path)?);
            }
            basename = Some(bname.to_string()); // assume
        }
        Ok(false)   /* do not skip */
    };
    
    for entry in read_dir(dir)? {
        // —— Get file name ——
        let entry = entry?;
        let fpath = &entry.path();
        let os_name = entry.file_name();    // must be named for lifetime
        let fname = match os_name.to_str() {
            Some(s) if s.ends_with(".pip") || s.ends_with(".piplog") => s,
            _ => { continue; },
        };
        
        // —— Match, filter and add ——
        if let Some(caps) = ss_pat.captures(fname) {
            let bname = caps.at(1).expect("cap");
            if filter_skip(bname, fpath)? { continue; }
            
            let ss: usize = caps.at(2).expect("cap").parse()?;
            trace!("Adding snapshot {}: {}", ss, fpath.display());
            let has_prev = part_paths.insert_ss(ss, entry.path());
            // #0011: better error handling
            assert!(!has_prev, "multiple files map to same basename/number");
        } else if let Some(caps) = cl_pat.captures(fname) {
            let bname = caps.at(1).expect("cap");
            if filter_skip(bname, fpath)? { continue; }
            
            let ss: usize = caps.at(2).expect("cap").parse()?;
            let cl: usize = caps.at(3).expect("cap").parse()?;
            trace!("Adding snapshot {} log {}: {}", ss, cl, fpath.display());
            let has_prev = part_paths.insert_cl(ss, cl, entry.path());
            // #0011: better error handling
            assert!(!has_prev, "multiple files map to same basename/number");
        } else {
            warn!(".pip or .piplog file does not match expected pattern: {}", fname);
            continue;
        }
    }
    }   // destroy filter_skip: end borrow on part_id and basename
    
    if let Some(mut bname) = basename {
        let part_id = part_id.expect("has part_id when has basename");
        if bname.ends_with('-') {
            // PartFileIO does not expect '-' separator in prefix
            bname.pop();
        }
        Ok((part_id, PartFileIO::new(dir.join(bname), part_paths)))
    } else {
        Err(Box::new(if opt_part_num.is_some() {
            PathError::new("discover::part_from_path: no files found matching part num in", path)
        } else {
            PathError::new("discover::part_from_path: no Pippin files found in", path)
        }))
    }
}

/// Will attempt to discover files belonging to a repository from a path.
/// 
/// The `path` argument is used to discover files. If it points to a directory,
/// this method will scan for all `.pip` and `.piplog` files in the directory
/// (optionally recursively).
/// If it points to a file, this method will proceed as if
/// it were run with the parent directory instead.
/// 
/// #0040: it would be nice to specify whether this should be recursive
/// (max_depth) and whether it should follow links, but without adding extra
/// required arguments (builder pattern like WalkDir?).
pub fn repo_from_path<P: AsRef<Path>>(path: P) -> Result<RepoFileIO> {
    let path = path.as_ref();
    let ss_pat = Regex::new("^((?:.*)-)?ss(0|[1-9][0-9]*)\\.pip$").expect("valid regex");
    let cl_pat = Regex::new("^((?:.*)-)?ss(0|[1-9][0-9]*)-cl(0|[1-9][0-9]*)\\.piplog$").expect("valid regex");
    enum Type { Snapshot(usize), Log(usize, usize) };
    
    let dir = if path.is_dir() {
        path
    } else if path.is_file() {
        path.parent().ok_or(PathError::new("unable to get parent dir", path))?
    } else {
        return PathError::err("neither a directory nor a file", path);
    };
    info!("Scanning for repository files in: {}", dir.display());
    
    // #0039: do we need to store the prefix as a string?
    let mut prefixes = HashMap::<String, PartId>::new();
    let mut partitions = HashMap::<PartId, PartPaths>::new();
    
    for entry in WalkDir::new(dir) {
        let entry = entry?;
        // filter non-Pippin files
        match entry.file_name().to_str() {
            Some(s) if s.ends_with(".pip") || s.ends_with(".piplog") => {}
            _ => { continue; }
        };
        
        let path = entry.path().to_path_buf();
        let (prefix, numbers) = {
            let rel_path = path.strip_prefix(dir)?.to_str().expect("path to str");
            if let Some(caps) = ss_pat.captures(rel_path) {
                (caps.at(1).expect("cap").to_string(), Type::Snapshot(
                    caps.at(2).expect("cap").parse()?))
            } else if let Some(caps) = cl_pat.captures(rel_path) {
                (caps.at(1).expect("cap").to_string(), Type::Log(
                    caps.at(2).expect("cap").parse()?,
                    caps.at(3).expect("cap").parse()?))
            } else {
                warn!(".pip or .piplog file does not match expected pattern: {}", rel_path);
                continue;
            }
        };
        
        let (pn, mut part_paths) = match prefixes.entry(prefix) {
            Entry::Occupied(e) => {
                let pn = e.get();
                (*pn, partitions.get_mut(pn).expect("partitions has entry for pn"))
            },
            Entry::Vacant(e) => {
                let fname = entry.file_name().to_str()
                    .ok_or_else(|| PathError::new("not valid UTF-8", path.to_path_buf()))?;
                let pn = find_part_num(fname, &path)?;
                e.insert(pn);
                (pn, partitions.entry(pn).or_insert_with(|| PartPaths::new()))
            },
        };
        
        match numbers {
            Type::Snapshot(ss) => {
                let has_prev = part_paths.insert_ss(ss, path);
                assert!(!has_prev, "Multiple files for partition number {}, \
                    snapshot {}", pn, ss);
            },
            Type::Log(ss, cl) => {
                let has_prev = part_paths.insert_cl(ss, cl, path);
                assert!(!has_prev, "Multiple files for partition number {}, \
                    snapshot {}, log {}", pn, ss, cl);
            }
        };
    }
    
    let mut repo = RepoFileIO::new(dir);
    for (mut prefix, pn) in prefixes {
        if let Some(part_files) = partitions.remove(&pn) {
            if prefix.ends_with('-') {
                // PartFileIO does not expect '-' separator in prefix
                prefix.pop();
            }
            repo.insert_part(pn, PartFileIO::new(dir.join(prefix), part_files));
        } else {
            // It is possible that multiple prefixes exist for the same
            // partition number, thus the part_files were already used
            assert!(repo.has_part(pn));
        }
    }
    Ok(repo)
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
            .and_then(|caps| caps.at(1).expect("cap")
            .parse().ok().map(|n| PartId::from_num(n)))
}   
/// A wrapper around `part_num_from_name` which discovers the number from the
/// name if possible and otherwise reads the header to find the partition
/// number. Fails if it can't find one.
/// 
/// `name`: full filename or basename, optionally prefixed by the path
/// `path`: full path to file
pub fn find_part_num<P: AsRef<Path>>(name: &str, path: P) -> Result<PartId> {
    if let Some(num) = part_num_from_name(name) {
        return Ok(num);
    }
    let head = read_head(&mut File::open(path)?)?;
    head.part_id.ok_or(Box::new(OtherError::new("file contains no part id")))
}
/// A helper to try matching a file name against standard Pippin file patterns,
/// and if it fits return the "basename" part.
pub fn discover_basename(fname: &str) -> Option<String> {
    let pat = Regex::new("^(.*)-ss(?:0|[1-9][0-9]*)(?:\\.pip|-cl(?:0|[1-9][0-9]*)\\.piplog)$")
            .expect("valid regex");
    
    pat.captures(fname)
            .map(|caps| caps.at(1).expect("cap").to_string())
}
