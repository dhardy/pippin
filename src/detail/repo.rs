//! Pippin's "repository" type and its dependencies
//! 
//! For simpler, single-partition usage, see the `partition` module. For full
//! functionality, use the `Repo` type in this module.
//! 
//! Implementations of the following traits are required for usage:
//! 
//! *   `RepoIO` with an accompanying `PartitionIO` to describe how to access
//!     the files (or other objects) storing data; the types in the `discover`
//!     module should suffice for normal usage
//! *   `ClassifierT` to classify elements, along with an `ElementT` type
//! *   `RepoT`. This type should handle partitioning, creation of `ClassifierT`
//!     objects, saving and discovering partitioning information, and provide
//!     the `RepoIO` implementation

use std::result;
use std::collections::HashMap;

// Re-export these. We pretend these are part of the same module while keeping files smaller.
pub use detail::repo_traits::{RepoIO, ClassifierT, ClassifyFallback, RepoT,
    RepoDivideError, DummyClassifier};
use partition::{Partition, PartitionState};
use detail::{EltId};
use PartId;
use error::{Result, OtherError, TipError, ElementOp};

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
pub struct Repo<C: ClassifierT, R: RepoT<C>> {
    /// Classifier. This must use compile-time polymorphism since it gives us
    /// the element type, and we do not want element look-ups to involve a
    /// run-time conversion.
    classifier: R,
    /// Descriptive identifier for the repository
    name: String,
    /// List of loaded partitions, by in-memory (temporary numeric) identifier.
    /// Identifier is TBD (TODO).
    partitions: HashMap<PartId, Partition<C::Element>>,
}

// Non-member functions on Repo
impl<C: ClassifierT, R: RepoT<C>> Repo<C, R> {
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
    pub fn create(mut classifier: R, name: String) -> Result<Repo<C, R>> {
        let (num, part_io) = try!(classifier.first_part());
        let part = try!(Partition::create_part(part_io, &name, num));
        let mut partitions = HashMap::new();
        partitions.insert(num, part);
        Ok(Repo{
            classifier: classifier,
            name: name,
            partitions: partitions,
        })
    }
    
    /// Open an existing repository.
    /// 
    /// This does not automatically load partition data, however it must load
    /// at least one header in order to identify the repository.
    pub fn open(classifier: R, io: Box<RepoIO>) -> Result<Repo<C, R>> {
        let mut part_nums = io.partitions().into_iter();
        let num0 = if let Some(num) = part_nums.next() {
            num
        } else {
            return OtherError::err("No repository files found");
        };
        
        let part_io = try!(io.make_partition_io(num0));
        let mut part0 = Partition::open(part_io, num0);
        let name = try!(part0.get_repo_name()).to_string();
        
        let mut parts = HashMap::new();
        parts.insert(num0, part0);
        for n in part_nums {
            let part_io = try!(io.make_partition_io(n));
            let mut part = Partition::open(part_io, n);
            try!(part.set_repo_name(&name));
            parts.insert(n, part);
        }
        
        Ok(Repo{
            classifier: classifier,
            name: name,
            partitions: parts,
        })
    }
}

// Member functions on Repo — a set of elements.
impl<C: ClassifierT, R: RepoT<C>> Repo<C, R> {
    /// Get the repo name
    pub fn name(&self) -> &str { &self.name }
    
    // TODO: some way to iterate or access partitions?
    
    /// Convenience function to call `Partition::load(all_history)` on all partitions.
    pub fn load_all(&mut self, all_history: bool) -> Result<()> {
        for (_, part) in &mut self.partitions {
            try!(part.load(all_history));
        }
        Ok(())
    }
    
    /// Convenience function to call `Partition::write(fast)` on all partitions.
    pub fn write_all(&mut self, fast: bool) -> Result<()> {
        for (_, part) in &mut self.partitions {
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
        for (_, part) in &mut self.partitions {
            all = all && part.unload(force);
        }
        all
    }
    
    /// Get a `RepoState` with a copy of the state of all loaded partitions.
    /// 
    /// This is not required for reading elements but is the only way to edit
    /// contents. Accessing the copy does not block operations on this `Repo`
    /// since the all shared state is reference-counted and immutable.
    /// 
    /// This operation is fairly cheap since elements are Copy-on-Write, but
    /// each partition's hash-map must still be copied.
    /// 
    /// The operation can fail if a partition requires merging.
    /// 
    /// TODO: a way to copy only some of the loaded partitions.
    pub fn clone_state(&self) -> result::Result<RepoState<C>, TipError> {
        let mut rs = RepoState::new(self.classifier.clone_classifier());
        for (num, part) in &self.partitions {
            rs.add_part(*num, try!(part.tip()).clone_exact());
        }
        Ok(rs)
    }
    
    /// Merge changes from a `RepoState` into the repo, consuming the
    /// `RepoState`.
    pub fn merge_in(&mut self, state: RepoState<C>) -> Result<()> {
        for (num, pstate) in state.states {
            let mut part = if let Some(p) = self.partitions.get_mut(&num) {
                p
            } else {
                panic!("partitions don't match!");
                //TODO: support for merging after a division/union/change of partitioning
            };
            if try!(part.push_state(pstate)) {
                if part.merge_required() {
                    panic!("merging not implemented");
                    //TODO — but how? *Always* require merge machinery to be passed to this method?
                }
            }
        }
        Ok(())
    }
    
    /// Merge changes from a `RepoState` and update it to the latest state of
    /// the `Repo`.
    pub fn sync(&mut self, _state: &mut RepoState<C>) {
        panic!("not implemented");  // TODO
    }
}

/// Provides read-write access to some or all partitions in a non-blocking
/// fashion. Has no access to historical states and is not able to load more
/// data on demand. Has to be merged back in to the repo in order to record
/// and synchronise edits.
pub struct RepoState<C: ClassifierT> {
    classifier: C,
    states: HashMap<PartId, PartitionState<C::Element>>,
}

impl<C: ClassifierT> RepoState<C> {
    /// Create new, with no partition states (use `add_part()`)
    fn new(classifier: C) -> RepoState<C> {
        RepoState { classifier: classifier, states: HashMap::new() }
    }
    /// Add a state from some partition
    fn add_part(&mut self, num: PartId, state: PartitionState<C::Element>) {
        self.states.insert(num, state);
    }
    
    /// Get a reference to some element (which can be cloned if required).
    /// 
    /// Note that elements can't be modified directly but must instead be
    /// replaced, hence there is no version of this function returning a
    /// mutable reference.
    pub fn get_elt(&self, id: EltId) -> Option<&C::Element> {
        //TODO: current policy is that the "partition" part of an element ID
        // is only there to ensure uniqueness. Can we not also use it for fast
        // look-ups?
//         let num = id.part_id();
        for (_num, state) in &self.states {
            match state.get_elt(id) {
                Some(e) => { return Some(e); },
                None => {},
            }
        }
        None
    }
    
    /// True if there are no elements within the state available to this `RepoState`
    pub fn is_empty(&self) -> bool {
        self.states.values().all(|v| v.is_empty())
    }
    
    /// Get the number of elements available to this `RepoState`
    pub fn num_elts(&self) -> usize {
        panic!("not implemented");
        //TODO: `sum()` is an unstable library feature:
//         self.states.values().map(|v| v.num_elts()).sum()
    }
    
    /// Insert an element and return the identifier, unless the id is already
    /// used in which case the function stops with an error.
    pub fn insert_elt(&mut self, id: EltId, elt: C::Element) -> Result<EltId, ElementOp> {
        let num = if let Some(num) = self.classifier.classify(&elt) {
            num
        } else {
            match self.classifier.fallback() {
                ClassifyFallback::Default(num) | ClassifyFallback::ReplacedOrDefault(num) => num,
                ClassifyFallback::ReplacedOrFail | ClassifyFallback::Fail => {
                    return Err(ElementOp::classify_failure());
                },
            }
        };
        if let Some(mut state) = self.states.get_mut(&num) {
            // Now insert into our PartitionState (may also fail):
            state.insert_elt(id, elt)
        } else {
            panic!("classifier did not give valid partition");  //TODO: handling
        }
    }
    /// Replace an existing element and return the replaced element, unless the
    /// id is not already used in which case the function stops with an error.
    pub fn replace_elt(&mut self, _id: EltId, _elt: C::Element) -> Result<C::Element, ElementOp> {
        panic!("not implemented");
        //TODO: how do we locate the element partition? (See get_elt().)
    }
    /// Remove an element, returning the element removed. If no element is
    /// found with the `id` given, `None` is returned.
    pub fn remove_elt(&mut self, _id: EltId) -> Result<C::Element, ElementOp> {
        panic!("not implemented");
        //TODO: how do we locate the element partition? (See get_elt().)
    }
}
