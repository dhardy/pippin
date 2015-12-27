//! Pippin "repository" type

use std::slice::{Iter, IterMut};
use std::any::Any;

use super::{Partition, PartitionIO};
use super::readwrite::validate_repo_name;
use ::Result;

pub trait RepoIO {
    /// Convert self to a `&Any`
    fn as_any(&self) -> &Any;
    
    /// Get the number of partitions found. Each `i` in `0 <= i < n` is a
    /// partition number (use with `make_partition_io()`).
    fn num_partitions(&self) -> usize;
    
    /// Add a new partition. TODO: some way to specify the path/base-name.
    /// 
    /// On success, returns the index of the new partition (for use with
    /// `make_partition_io()`).
    fn add_partition(&mut self) -> usize;
    
    /// Construct and return a new PartitionIO for partition `n`.
    fn make_partition_io(&self, n: usize) -> Box<PartitionIO>;
}

/// Handle on a repository.
/// 
/// A repository can be created... TODO
/// 
/// Elements of a repository can be retrieved in a read-only fashion by
/// specifying a partition identifier and element identifier, or elements can
/// be searched for via various criteria TODO. These operations block access to
/// the in-memory copy of the repository during their usage.
/// 
/// Additionally, a copy of the current state of a partition can be retrieved
/// and used to read and write elements. The copy may be accessed without
/// blocking other operations on the underlying repository. Changes made to
/// the copy may be merged back into the repository.
pub struct Repo {
    io: Box<RepoIO>,
    /// TODO: what is this for?
    name: String,
    /// List of loaded partitions, by in-memory (temporary numeric) identifier.
    /// Identifier is TBD (TODO).
    partitions: Vec<Partition>,
}

// Non-member functions on Repo
impl Repo {
    /// Create a new repository with the given name
    pub fn new(mut io: Box<RepoIO>, name: String) -> Result<Repo> {
        try!(validate_repo_name(&name));
        let n = io.add_partition();
        let part = try!(Partition::new(io.make_partition_io(n), &name));
        Ok(Repo{
            io: io,
            name: name,
            partitions: vec![part],
        })
    }
    
    /// Open an existing repository.
    /// 
    /// This does not automatically load partition data.
    pub fn open(io: Box<RepoIO>) -> Result<Repo> {
        let n = io.num_partitions();
        let mut parts = Vec::with_capacity(n);
        for i in 0..n {
            parts.push(Partition::create(io.make_partition_io(i)));
            //TODO: should this read a header and verify a repository identifier and/or partition identifier?
        }
        Ok(Repo{
            io: io,
            name: "".to_string() /*TODO*/,
            partitions: parts,
        })
    }
}

// Member functions on Repo — a set of elements.
impl Repo {
    /// Get the repo name
    pub fn name(&self) -> &str { &self.name }
    
    /// Get an iterator over partitions
    pub fn partitions(&self) -> Iter<Partition> {
        self.partitions.iter()
    }
    
    /// Get a mutable iterator over partitions
    pub fn partitions_mut(&mut self) -> IterMut<Partition> {
        self.partitions.iter_mut()
    }
    
    /// Convenience function to call `Partition::load(all_history)` on all partitions.
    pub fn load_all(&mut self, all_history: bool) -> Result<()> {
        for part in &mut self.partitions {
            try!(part.load(all_history));
        }
        Ok(())
    }
    
    /// Convenience function to call `Partition::write(fast)` on all partitions.
    pub fn write_all(&mut self, fast: bool) -> Result<()> {
        for part in &mut self.partitions {
            try!(part.write(fast));
        }
        Ok(())
    }
    
    /// Convenience function to call `Partition::unload(force)` on all partitions.
    /// 
    /// If `force == true`, all data is unloaded (without saving any changes)
    /// and `true` is returned. If `force == false`, partitions with no unsaved
    /// changes are unloaded while others are left unchanged. `true` is returned
    /// if and only if all partitions are unloaded.
    pub fn unload_all(&mut self, force: bool) -> bool {
        let mut all = true;
        for part in &mut self.partitions {
            all = all && part.unload(force);
        }
        all
    }
}
