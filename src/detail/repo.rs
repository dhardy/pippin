//! Pippin "repository" type

use std::slice::{Iter, IterMut};

use super::{Partition, ClassifierT, RepoIO};
use ::error::{Result, OtherError};

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
pub struct Repo<C: ClassifierT> {
    /// Classifier. This must use compile-time polymorphism since it gives us
    /// the element type, and we do not want element look-ups to involve a
    /// run-time conversion.
    classifier: C,
    /// Descriptive identifier for the repository
    name: String,
    /// List of loaded partitions, by in-memory (temporary numeric) identifier.
    /// Identifier is TBD (TODO).
    partitions: Vec<Partition<C::Element>>,
}

// Non-member functions on Repo
impl<C: ClassifierT> Repo<C> {
    /// Create a new repository with the given name.
    /// 
    /// The name must be UTF-8 and not more than 16 bytes long. It allows a
    /// user-friendly description of the repository to appear in each data
    /// file. It may also be useful for each repository to have a unique name
    /// in order to differentiate files (this name is verified on each file
    /// read).
    /// 
    /// This creates an initial 'partition' ready for use (all contents must
    /// be kept within a `Partition`).
    pub fn create(mut classifier: C, name: String) -> Result<Repo<C>> {
        let (num, part_io) = try!(classifier.first_part());
        let part = try!(Partition::create_part(part_io, &name, num));
        Ok(Repo{
            classifier: classifier,
            name: name,
            partitions: vec![part],
        })
    }
    
    /// Open an existing repository.
    /// 
    /// This does not automatically load partition data, however it must load
    /// at least one header in order to identify the repository.
    pub fn open(classifier: C, io: Box<RepoIO>) -> Result<Repo<C>> {
        let part_nums = io.partitions();
        if part_nums.is_empty() {
            return OtherError::err("No repository files found");
        }
        
        let part_io = try!(io.make_partition_io(part_nums[0]));
        let mut part0 = Partition::open(part_io);
        let name = try!(part0.get_repo_name()).to_string();
        
        let mut parts = Vec::with_capacity(part_nums.len());
        parts.push(part0);
        for i in 1..part_nums.len() {
            let part_io = try!(io.make_partition_io(part_nums[i]));
            let mut part = Partition::open(part_io);
            try!(part.set_repo_name(&name));
            parts.push(part);
        }
        
        Ok(Repo{
            classifier: classifier,
            name: name,
            partitions: parts,
        })
    }
}

// Member functions on Repo — a set of elements.
impl<C: ClassifierT> Repo<C> {
    /// Get the repo name
    pub fn name(&self) -> &str { &self.name }
    
    /// Get an iterator over partitions
    pub fn partitions(&self) -> Iter<Partition<C::Element>> {
        self.partitions.iter()
    }
    
    /// Get a mutable iterator over partitions
    pub fn partitions_mut(&mut self) -> IterMut<Partition<C::Element>> {
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
