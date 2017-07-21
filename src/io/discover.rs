/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Pippin: file discovery

use std::path::Path;
use std::fs::read_dir;

use regex::Regex;

use io::file::{PartFileIO, PartPaths};
use error::{Result, PathError};


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
pub fn part_from_path<P: AsRef<Path>>(path: P) -> Result<PartFileIO> {
    let path = path.as_ref();
    let ss_pat = Regex::new("^((?:.*)-)?ss(0|[1-9][0-9]*)\\.pip$").expect("valid regex");
    let cl_pat = Regex::new("^((?:.*)-)?ss(0|[1-9][0-9]*)-cl(0|[1-9][0-9]*)\\.piplog$").expect("valid regex");
    
    let mut basename: Option<String> = None;
    
    let dir = if path.is_dir() {
        info!("Scanning for partition files in: {}", path.display());
        path
    } else if let Some(fname) = path.file_name() {
        let fname = fname.to_str().ok_or_else(|| PathError::new("not valid UTF-8", path))?;
        if let Some(bname) = discover_basename(fname) {
            let dir = path.parent().ok_or_else(|| PathError::new("path has no parent", path))?;
            info!("Scanning for partition files matching: {}/{}*", dir.display(), bname);
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
    let mut filter_skip = |bname: &str| -> Result<bool> {
        if let Some(ref req_bname) = basename {
            // basename known: filter by it
            if bname != req_bname {
                return Ok(true);    // skip
            }
        }
        // done filtering; update basename if necessary
        if basename == None {
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
            if filter_skip(bname)? { continue; }
            
            let ss: usize = caps.at(2).expect("cap").parse()?;
            trace!("Adding snapshot {}: {}", ss, fpath.display());
            let has_prev = part_paths.insert_ss(ss, entry.path());
            // #0011: better error handling
            assert!(!has_prev, "multiple files map to same basename/number");
        } else if let Some(caps) = cl_pat.captures(fname) {
            let bname = caps.at(1).expect("cap");
            if filter_skip(bname)? { continue; }
            
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
    }   // destroy filter_skip: end borrow on basename
    
    if let Some(mut bname) = basename {
        if bname.ends_with('-') {
            // PartFileIO does not expect '-' separator in prefix
            bname.pop();
        }
        Ok(PartFileIO::for_paths(dir.join(bname), part_paths))
    } else {
        Err(Box::new(PathError::new("discover::part_from_path: no Pippin files found in", path)))
    }
}


/// A helper to try matching a file name against standard Pippin file patterns,
/// and if it fits return the "basename" part.
pub fn discover_basename(fname: &str) -> Option<String> {
    let pat = Regex::new("^(.*)-ss(?:0|[1-9][0-9]*)(?:\\.pip|-cl(?:0|[1-9][0-9]*)\\.piplog)$")
            .expect("valid regex");
    
    pat.captures(fname)
            .map(|caps| caps.at(1).expect("cap").to_string())
}
