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
    use std::fs::DirBuilder;
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
    /// Get absolute path to some data object
    pub fn get_data_dir<P: AsRef<Path>>(name: P) -> PathBuf {
        let mut p = get_top_dir();
        p.push(name);
        assert!(p.exists());
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
}


/// Sequences example: type and generators
pub mod seq;
