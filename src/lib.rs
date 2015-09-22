//! Pippin (sync-sets) library

use std::io;
use std::collections::HashMap;

// Method of providing partition data
pub trait DataResource {
    // This function should return some kind of data stream given a name. The
    // name might for example be a file name minus extension under some path.
    fn resolve(&mut self, name: &str) -> io::Read;
    
    // This function should create a writer to write to a new file (or other
    // store). The Write object will be destroyed after use.
    fn write(&mut self, name: &str) -> io::Write;
    
    // This function should open a file/store for appending. If the file does
    // not exist or its current length is not that given, the operation should
    // fail.
    fn append(&mut self, name: &str, len: u64) -> io::Result<io::Write>;
}

// Handle on a repository
pub struct Repo {
    dummy: i32
}

// Non-member functions on Repo
impl Repo {
    // Load a repo from disk
    pub fn load(res: &mut DataResource) -> Repo {
        Repo{dummy:0}
    }
    
    // Create a new repo
    pub fn new() -> Repo {
        Repo{dummy:0}
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
    // TODO: list all partitions
    // TODO: check whether a particular partition is loaded
    
    // Unload all partitions, saving changes to disk
    pub fn unload_all(&mut self) {}
    
    // Commit all changes to disk
    pub fn commit_all(&mut self) {}
}

// A *partition* is a sub-set of the entire set such that (a) each element is
// in exactly one partition, (b) a partition is small enough to be loaded into
// memory in its entirety, (c) there is some user control over the number of
// partitions and how elements are assigned partitions and (d) each partition
// can be managed independently of other partitions.
//
// Partitions are the *only* method by which the entire set may grow beyond
// available memory, thus smart allocation of elements to partitions will be
// essential for some use-cases.
struct Partition {
    // NOTE: with sequential keys VecMap could be an alternative to HashMap
    elements: HashMap<u64, Element>
}

// Partition functions
impl Partition {
    // Unload partition, saving changes to disk
    pub fn unload(&mut self) {}
    
    // Commit changes to disk
    pub fn commit(&mut self) {}
    
    // Enumerate all item identifiers
    // TODO
    
    // Get an item as a byte vector. Panics on invalid item_id.
    pub fn get_item(&self, item_id: u64) -> &Element {
        self.elements.get(&item_id).unwrap()
    }
    
    // List all items in full (or through some filter)
    // TODO
    
    // Search for some item
    // TODO
}

// Holds an element's data in memory
// TODO: replace with a trait and user-defined implementation?
struct Element;

