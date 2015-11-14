//! Pippin (sync-sets) library

// because at this stage of development there's a lot of it:
#![allow(dead_code)]

// https://github.com/rust-lang/rust/issues/27790
// could be replaced with a stub function if needing to use stable releases
#![feature(vec_resize)]

// Used for error display; not essential
#![feature(step_by)]

#![feature(box_syntax)]

// Using this until PathExt is folded in...
#![feature(path_ext)]

extern crate crypto;
extern crate chrono;
extern crate byteorder;
extern crate hashindexed;
extern crate regex;
extern crate vec_map;

use std::{io, fs};
use std::collections::hash_map::{Keys};
use std::path::Path;
use std::convert::AsRef;

use detail::{FileHeader, FileType, read_head, write_head, validate_repo_name};
use detail::{read_snapshot, write_snapshot};

pub use detail::{Element};
pub use detail::{PartitionState};
pub use detail::{Partition, PartitionIO, PartitionDummyIO, TipError};
pub use detail::DiscoverPartitionFiles;
pub use error::{Error, Result};

pub mod error;
mod detail;

/// Version. The low 16 bits are patch number, next 16 are the minor version
/// number, the next are the major version number. The top 16 are zero.
pub const LIB_VERSION: u64 = 0x0000_0000_0000;

/// Handle on a repository
pub struct Repo {
    name: String,
    state: PartitionState
}

// Non-member functions on Repo
impl Repo {
    /// Create a new repo with the given name
    pub fn new(name: String) -> Result<Repo> {
        try!(validate_repo_name(&name));
        Ok(Repo{
            name: name,
            state: PartitionState::new()
        })
    }
    
    /// Load a snapshot from a stream
    pub fn load_stream(stream: &mut io::Read) -> Result<Repo> {
        let head = try!(read_head(stream));
        let state = try!(read_snapshot(stream));
        //TODO: could check that we're at the end of the stream (?)
        
        Ok(Repo {
            name: head.name,
            state: state
        })
    }
    
    /// Load a snapshot from a file
    pub fn load_file<P: AsRef<Path>>(p: P) -> Result<Repo> {
        let mut f = try!(fs::File::open(p));
        Repo::load_stream(&mut f)
    }
    
    /// Save a snapshot to a stream
    pub fn save_stream(&self, stream: &mut io::Write) -> Result<()> {
        let head = FileHeader {
            ftype: FileType::Snapshot,
            name: self.name.clone(),
            remarks: vec![],
            user_fields: vec![]
        };
        
        try!(write_head(&head, stream));
        write_snapshot(&self.state, stream)
    }
    
    /// Save a snapshot to a file
    pub fn save_file<P: AsRef<Path>>(&self, p: P) -> Result<()> {
        let mut f = try!(fs::File::create(p));
        self.save_stream(&mut f)
    }
}

// Member functions on Repo — a set of elements.
//
// Each element of the set has a unique identifier and some data. This Repo
// stores the elements along with a history of their changes.
//
// This data store is optimised for the case where elements only have a small
// amount of data, verification of data integrity is important and disk writes
// should be minimised. It is designed to scale beyond available memory via
// partitioning and to allow simple backup as well as recovery of as much data
// as possible in the case that some information is lost. It is also designed
// to enable synchronisation of the data set between multiple computers over
// even low-speed network connections, such that all computers have a full
// local copy of the data and its history.
impl Repo {
    /// Get the repo name
    pub fn name(&self) -> &str { &self.name }
    
    /// Get the number of elements
    pub fn num_elts(&self) -> usize {
        self.state.num_elts()
    }
    
    /// Return true iff this element exists
    pub fn has_elt(&self, id: u64) -> bool {
        self.state.has_elt(id)
    }
    
    /// Get an iterator over element identifiers
    pub fn element_ids(&self) -> Keys<u64, Element> {
        self.state.elt_ids()
    }
    
    /// Get an element's data. Returns None if the specified element does not
    /// exist.
    pub fn get_element(&self, id: u64) -> Option<&Element> {
        self.state.get_elt(id)
    }
    
    /// Insert an element and return (), unless the id is already used in
    /// which case the function stops with an error.
    pub fn insert_elt(&mut self, id: u64, elt: Element) -> Result<()> {
        self.state.insert_elt(id, elt)
    }
    
    // TODO: list all partitions
    // TODO: check whether a particular partition is loaded
    
    // Unload all partitions, saving changes to disk
    pub fn unload_all(&mut self) {}
    
    // Commit all changes to disk
    pub fn commit_all(&mut self) {}
}
