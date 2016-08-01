 /* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#[macro_use]
extern crate log;
extern crate rand;
extern crate byteorder;
extern crate mktemp;
#[macro_use(try_read)]
extern crate pippin;


/// Utility functions used by tests. These panic on error for simplicity.
pub mod util {
    use std::env;
    use std::path::{Path, PathBuf};
    use std::ffi::OsStr;
    use std::fs::{self, File, DirBuilder};
    use std::io::{self, Read};
    use std::collections::hash_map::{HashMap, Entry};
    use mktemp::Temp;
    use rand::{ChaChaRng, SeedableRng};
    
    /// Get absolute path to the "target" directory ("build" dir)
    pub fn get_target_dir() -> PathBuf {
        let bin = env::current_exe().expect("exe path");
        let mut dir = PathBuf::from(bin.parent().expect("bin parent"));
        while dir.file_name() != Some(OsStr::new("target")) {
            dir.pop();
        }
        assert!(dir.is_dir());
        dir
    }
    /// Get absolute path to the (sub-)project's top dir
    pub fn get_top_dir() -> PathBuf {
        let mut dir = get_target_dir();
        let success = dir.pop();
        assert!(success);
        assert!(dir.is_dir());
        dir
    }
    /// Get absolute path to some data object; this may or may not exist
    pub fn get_data_dir<P: AsRef<Path>>(name: P) -> PathBuf {
        let mut p = get_top_dir();
        p.push("data");
        p.push(name);
        p
    }
    /// Get absolute path to a new temporary directory under target.
    /// 
    /// `name` is incorporated into the path somehow (currently by making a
    /// directory with this name).
    pub fn mk_temp_dir(name: &str) -> Temp {
        let mut dir = get_target_dir();
        dir.push("tmp");
        if name.len() > 0 { dir.push(name); }
        DirBuilder::new().recursive(true).create(&dir).expect("create_dir");
        assert!(dir.is_dir());
        Temp::new_dir_in(&dir).expect("new temp dir")
    }
    
    /// Make a deterministic random number generator with a given seed.
    /// 
    /// (This doesn't need to be cryptographically secure, but there aren't
    /// many generators available...)
    pub fn mk_rng(seed: u32) -> ChaChaRng {
        ChaChaRng::from_seed(&[seed])
    }
    
    /// Test whether two files are the same. Returns true if they are.
    pub fn files_are_eq<P1: AsRef<Path>, P2: AsRef<Path>>(p1: P1, p2: P2) -> io::Result<bool> {
        let mut f1 = try!(File::open(p1));
        let mut f2 = try!(File::open(p2));
        
        const BUF_SIZE: usize = 4096;   // common page size on Linux
        let mut buf1 = [0u8; BUF_SIZE];
        let mut buf2 = [0u8; BUF_SIZE];
        
        loop {
            let len = try!(f1.read(&mut buf1));
            if len == 0 {
                // EOF of f1; is f2 also at EOF?
                let l2 = try!(f2.read(&mut buf2[0..1]));
                return Ok(l2 == 0);
            }
            match f2.read_exact(&mut buf2[0..len]) {
                Ok(()) => {},
                Err(e) => {
                    if e.kind() == io::ErrorKind::UnexpectedEof {
                        return Ok(false);
                    }
                    return Err(e);
                },
            }
            if buf1[0..len] != buf2[0..len] {
                return Ok(false);
            }
        }
    }
    
    /// Test whether two paths are the same, recursing over directories.
    /// 
    /// Links are followed (the pointed objects compared); broken links result
    /// in an error with kind `ErrorKind::NotFound`.
    pub fn paths_are_eq<P1: AsRef<Path>, P2: AsRef<Path>>(p1: P1, p2: P2) -> io::Result<bool>
    {
        #[derive(PartialEq)]
        enum Cat { File, Dir }
        let classify = |p: &Path| {
            if p.is_file() {
                Ok(Cat::File)
            } else if p.is_dir() {
                Ok(Cat::Dir)
            } else {
                Err(io::Error::new(io::ErrorKind::NotFound, "broken symlink or unknown object"))
            }
        };
        let cat1 = try!(classify(p1.as_ref()));
        let cat2 = try!(classify(p2.as_ref()));
        if cat1 != cat2 {
            return Ok(false);
        }
        
        match cat1 {
            Cat::File => {
                files_are_eq(p1, p2)
            },
            Cat::Dir => {
                let mut entries = HashMap::new();
                for entry in try!(fs::read_dir(p1)) {
                    let entry = try!(entry);
                    let name = entry.file_name();
                    assert!(!entries.contains_key(&name));
                    entries.insert(name, (entry.path(), None));
                }
                for dir_entry in try!(fs::read_dir(p2)) {
                    let dir_entry = try!(dir_entry);
                    match entries.entry(dir_entry.file_name()) {
                        Entry::Occupied(mut e) => {
                            assert!(e.get().1 == None); // not already set
                            e.get_mut().1 = Some(dir_entry.path());
                        },
                        Entry::Vacant(_) => {
                            return Ok(false);   // missing from dir p1
                        },
                    };
                }
                // Check for missing entries from dir p2 next, so we don't
                // recurse unnecessarily:
                if !entries.values().all(|ref v| v.1.is_some()) {
                    return Ok(false);
                }
                // Now recurse over members of p1 and p2:
                for ref v in entries.values() {
                    let pe1 = &v.0;
                    if let &Some(ref pe2) = &v.1 {
                        if !try!(paths_are_eq(&pe1, &pe2)) {
                            return Ok(false);
                        }
                    } else { assert!(false); }
                }
                Ok(true)
            },
        }
    }
    
    #[test]
    fn test_path_diff() {
        let top = get_top_dir();
        let src = top.join("src");
        let none = top.join("nonexistant");
        let result = paths_are_eq(&src, &src).expect("paths_are_eq");
        assert_eq!(result, true);
        assert_eq!(paths_are_eq(&none, &src).unwrap_err().kind(), io::ErrorKind::NotFound);
        let result = paths_are_eq(&src, &top).expect("paths_are_eq");
        assert_eq!(result, false);
    }
}


/// Sequences example: type and generators
pub mod seq;
