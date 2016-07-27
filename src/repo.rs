/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Pippin's "repository" type and its dependencies
//! 
//! For simpler, single-partition usage, see the `partition` module. For full
//! functionality, use the `Repository` type in this module.
//! 
//! Implementations of the following traits are required for usage:
//! 
//! *   `RepoIO` with an accompanying `PartIO` to describe how to access
//!     the files (or other objects) storing data; the types in the `discover`
//!     module should suffice for normal usage
//! *   `ClassifierT` to classify elements, along with an `ElementT` type
//! *   `RepoT`. This type should handle partitioning, creation of `ClassifierT`
//!     objects, saving and discovering partitioning information, and provide
//!     the `RepoIO` implementation

use std::result;
use std::collections::HashMap;
use std::rc::Rc;
use std::mem::swap;

// Re-export these. We pretend these are part of the same module while keeping files smaller.
pub use repo_traits::{RepoIO, ClassifierT, ClassifyFallback, RepoT,
    RepoDivideError, DummyClassifier};
use {Partition, StateT, MutStateT, MutPartState};
use merge::TwoWaySolver;
use {EltId, PartId};
use commit::MakeMeta; 
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
pub struct Repository<C: ClassifierT, R: RepoT<C>> {
    /// Classifier. This must use compile-time polymorphism since it gives us
    /// the element type, and we do not want element look-ups to involve a
    /// run-time conversion.
    classifier: R,
    /// Descriptive identifier for the repository
    name: String,
    /// List of loaded partitions, by their `PartId`.
    partitions: HashMap<PartId, Partition<C::Element>>,
}

// Non-member functions on Repository
impl<C: ClassifierT, R: RepoT<C>> Repository<C, R> {
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
    pub fn create<S: Into<String>>(mut classifier: R, name: S,
            make_meta: Option<&MakeMeta>) -> Result<Repository<C, R>>
    {
        let name: String = name.into();
        info!("Creating repository: {}", name);
        let part_io = try!(classifier.init_first());
        let part = try!(Partition::create(part_io, &name, Some(&mut classifier), make_meta));
        let mut partitions = HashMap::new();
        partitions.insert(part.part_id(), part);
        Ok(Repository{
            classifier: classifier,
            name: name,
            partitions: partitions,
        })
    }
    
    /// Open an existing repository.
    /// 
    /// This does not automatically load partition data, however it must load
    /// at least one header in order to identify the repository.
    pub fn open(mut classifier: R)-> Result<Repository<C, R>> {
        let (name, parts) = {
            let io = classifier.repo_io();
            let mut part_nums = io.parts().into_iter();
            let num0 = if let Some(num) = part_nums.next() {
                num
            } else {
                return OtherError::err("No repository files found");
            };
            
            let part_io = try!(io.make_part_io(num0));
            let mut part0 = try!(Partition::open(part_io));
            let name = try!(part0.get_repo_name()).to_string();
            
            let mut parts = HashMap::new();
            parts.insert(num0, part0);
            for n in part_nums {
                let part_io = try!(io.make_part_io(n));
                let mut part = try!(Partition::open(part_io));
                try!(part.set_repo_name(&name));
                parts.insert(n, part);
            }
            (name, parts)
        };
        
        info!("Opening repository with {} partitions: {}", parts.len(), name);
        Ok(Repository{
            classifier: classifier,
            name: name,
            partitions: parts,
        })
    }
}

// Member functions on Repository — a set of elements.
impl<C: ClassifierT, R: RepoT<C>> Repository<C, R> {
    /// Get the repo name
    pub fn name(&self) -> &str { &self.name }
    
    // TODO: some way to iterate or access partitions?
    
    /// Load the latest state of all partitions
    pub fn load_latest(&mut self, make_meta: Option<&MakeMeta>) -> Result<()> {
        for (_, part) in &mut self.partitions {
            try!(part.load_latest(Some(&mut self.classifier), make_meta));
        }
        Ok(())
    }
    
    /// Write commits to the disk for all partitions.
    /// 
    /// If `fast` is true, "maintenance" operations (writing snapshot files)
    /// are suppressed.
    pub fn write_all(&mut self, fast: bool) -> Result<()> {
        for (_, part) in &mut self.partitions {
            try!(part.write(fast, Some(&mut self.classifier)));
        }
        Ok(())
    }
    
    /// Force all loaded partitions to write a snapshot.
    pub fn write_snapshot_all(&mut self) -> Result<()> {
        for (_, part) in &mut self.partitions {
            try!(part.write_snapshot(Some(&mut self.classifier)));
        }
        Ok(())
    }
    
    /// Call `Partition::unload(force)` on all partitions.
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
    
    /// Returns true if any merge is required. This may be required after
    /// `merge_in()` or `sync()` is called, and can also be needed after
    /// loading data from an external resource.
    /// 
    /// When this returns true, `merge()` should be called before further
    /// action.
    pub fn merge_required(&self) -> bool {
        self.partitions.values().any(|p| p.merge_required())
    }
    
    /// Does any merge work requried.
    /// 
    /// Note that this is not the same as `merge_in()`, which integrates
    /// changes from a `RepoState` but does not do low-level merge work (if
    /// required). This function does the low-level merging.
    /// 
    /// If no merge work is required and you have your solver ready, calling
    /// this should be roughly as efficient as calling `merge_required()`.
    /// 
    /// If `auto_load` is true, additional history will be loaded as necessary
    /// to find a common ancestor.
    /// 
    /// TODO: clearer names, maybe move some of the work around.
    pub fn merge<S: TwoWaySolver<C::Element>>(&mut self, solver: &S,
            auto_load: bool, make_meta: Option<&MakeMeta>) -> Result<()>
    {
        for (_, part) in &mut self.partitions {
            try!(part.merge(solver, auto_load, make_meta));
        }
        Ok(())
    }
    
    /// Get a `RepoState` with a copy of the state of all loaded partitions.
    /// 
    /// This is not required for reading elements but is the only way to edit
    /// contents. Accessing the copy does not block operations on this `Repository`
    /// since the all shared state is reference-counted and immutable.
    /// 
    /// This operation is fairly cheap since elements are Copy-on-Write, but
    /// each partition's hash-map must still be copied.
    /// 
    /// The operation can fail if a partition requires merging. Partitions not
    /// loaded are omitted from the resulting `RepoState`.
    /// 
    /// TODO: a way to copy only some of the loaded partitions.
    pub fn clone_state(&self) -> result::Result<RepoState<C>, TipError> {
        let mut rs = RepoState::new(self.classifier.clone_classifier());
        for (num, part) in &self.partitions {
            if part.is_loaded() {
                rs.add_part(*num, try!(part.tip()).clone_mut());
            }
        }
        Ok(rs)
    }
    
    /// Merge changes from a `RepoState` into the repo, consuming the
    /// `RepoState`.
    /// 
    /// Returns true when any further merge work is required. In this case
    /// `merge()` should be called.
    pub fn merge_in(&mut self, state: RepoState<C>,
            make_meta: Option<&MakeMeta>) -> Result<bool>
    {
        let mut merge_required = false;
        for (num, pstate) in state.states {
            let mut part = if let Some(p) = self.partitions.get_mut(&num) {
                p
            } else {
                panic!("RepoState has a partition not found in the Repository");
                //TODO: support for merging after a division/union/change of partitioning
            };
            if try!(part.push_state(pstate, make_meta)) {
                if part.merge_required() { merge_required = true; }
            }
        }
        Ok(merge_required)
    }
    
    /// Merge changes from a `RepoState` and update it to the latest state of
    /// the `Repository`.
    /// 
    /// Returns true if further merge work is required. In this case, `merge()`
    /// should be called on the `Repository`, then `sync()` again (until then, the
    /// `RepoState` will have no access to any partitions with conflicts).
    pub fn sync(&mut self, state: &mut RepoState<C>,
            make_meta: Option<&MakeMeta>) -> Result<bool>
    {
        let mut states = HashMap::new();
        swap(&mut states, &mut state.states);
        
        let mut merge_required = false;
        for (num, pstate) in states {
            let mut part = match self.partitions.get_mut(&num) {
                Some(p) => p,
                None => {
                    panic!("RepoState has a partition not found in the Repository");
                    //TODO: support for merging after a division/union/change of partitioning
                },
            };
            //TODO: if equal to partition tip, do nothing... but we can't test
            // this now so can't short-cut — reimplement this or forget it?
            /*if let Ok(sum) = part.tip_key() {
                if sum == pstate.statesum() {
                    // (#0022: Presumably) no changes. Skip partition.
                    state.add_part(num, pstate);
                    continue;
                }
            }*/
            if try!(part.push_state(pstate, make_meta)) {
                if part.merge_required() {
                    merge_required = true;
                } else {
                    state.add_part(num, try!(part.tip()).clone_mut());
                }
            }
        }
        
        for (num, part) in &self.partitions {
            if !state.has_part(*num) {
                state.add_part(*num, try!(part.tip()).clone_mut());
            }
        }
        Ok(merge_required)
    }
}

/// Provides read-write access to some or all partitions in a non-blocking
/// fashion. This does not know about any partitions not internally available,
/// has no access to historical states and is not able to load more
/// data on demand.
/// 
/// This should be merged back in to the repo in order to record and
/// synchronise edits.
pub struct RepoState<C: ClassifierT> {
    classifier: C,
    states: HashMap<PartId, MutPartState<C::Element>>,
}

impl<C: ClassifierT> RepoState<C> {
    /// Create new, with no partition states (use `add_part()`)
    fn new(classifier: C) -> RepoState<C> {
        RepoState { classifier: classifier, states: HashMap::new() }
    }
    /// Add a state from some partition
    fn add_part(&mut self, num: PartId, state: MutPartState<C::Element>) {
        self.states.insert(num, state);
    }
    /// Checks whether the given partition is present
    pub fn has_part(&self, num: PartId) -> bool {
        self.states.contains_key(&num)
    }
    /// Counts the number of partitions represented
    pub fn num_parts(&self) -> usize {
        self.states.len()
    }
    
    /// Find an element that may have moved. This method returns an EltId on
    /// success which can then be used by other methods (`get()`, etc.).
    /// 
    /// If the element has not been moved and its partition is loaded, this
    /// will return the same identifier and be fast.
    /// 
    /// If the element's partition is not loaded, this will fail, since a
    /// `RepoState` cannot load partitions. It will normally indicate which
    /// partition should be loaded, however without checking the partition it
    /// cannot be sure that this is correct.
    /// 
    /// This may also fail completely. In this case searching all partitions
    /// may still find the element (either use `Repository::locate(...)` or
    /// `Repository::load_all()` then call this again on a fresh `RepoState` or after
    /// synchronising). This method does search all loaded partitions when
    /// other strategies fail.
    pub fn locate(&mut self, mut id: EltId) -> Result<EltId, ElementOp> {
        let mut to_update = Vec::<EltId>::new();
        loop {
            let part_id = id.part_id();
            if let Some(state) = self.states.get(&part_id) {
                if state.is_avail(id) {
                    // Partition is loaded and has element
                    /*TODO: should we do this? Need to resolve lifetime issue if so.
                    if to_update.len() > 1 {
                        // Update notes in loaded partitions, excepting the last
                        // which is already correct:
                        to_update.pop();
                        for old_id in to_update{
                            let part_id = old_id.part_id();
                            if let Some(mut state) = self.states.get_mut(&part_id) {
                                state.set_move(old_id, id);
                            }
                        }
                    }*/
                    return Ok(id);
                } else if let Some(new_id) = state.is_moved(id) {
                    // We have a new lead, check whether the element is in fact
                    // there. Remember this note.
                    to_update.push(id);
                    id = new_id;
                    continue;
                }
                // else: Partition is loaded but does not have element!
            } else {
                return Err(ElementOp::NotLoaded);
            }
            break;
        }
        
        // We didn't find the element. In this case we could naively
        // search all partitions, however if so it would have a new identifier.
        // We *could* try finding another element with the same `elt_num()`,
        // but we might find the wrong element in this case (and could also
        // miss the element we are looking for, since it might have a new num).
        // TODO: should elements remember their old names?
        
        // No success; fail
        Err(ElementOp::NotFound)
    }
}

impl<C: ClassifierT> StateT<C::Element> for RepoState<C> {
    fn any_avail(&self) -> bool {
        self.states.values().any(|v| v.any_avail())
    }
    fn num_avail(&self) -> usize {
        self.states.values().fold(0, |acc, ref v| acc + v.num_avail())
    }
    fn is_avail(&self, id: EltId) -> bool {
        let part_id = id.part_id();
        self.states.get(&part_id).map_or(false, |state| state.is_avail(id))
    }
    fn get_rc(&self, id: EltId) -> Result<&Rc<C::Element>, ElementOp> {
        let part_id = id.part_id();
        match self.states.get(&part_id) {
            Some(state) => state.get_rc(id),
            None => Err(ElementOp::NotLoaded),
        }
    }
}
impl<C: ClassifierT> MutStateT<C::Element> for RepoState<C> {
    fn insert_rc_initial(&mut self, initial: u32, elt: Rc<C::Element>) -> Result<EltId, ElementOp> {
        let part_id = if let Some(part_id) = self.classifier.classify(&*elt) {
            part_id
        } else {
            match self.classifier.fallback() {
                ClassifyFallback::Default(part_id) | ClassifyFallback::ReplacedOrDefault(part_id) => part_id,
                ClassifyFallback::ReplacedOrFail | ClassifyFallback::Fail => {
                    return Err(ElementOp::ClassifyFailure);
                },
            }
        };
        if let Some(mut state) = self.states.get_mut(&part_id) {
            // Now insert into our PartState (may also fail):
            state.insert_rc_initial(initial, elt)
        } else {
            Err(ElementOp::NotLoaded)
        }
    }
    fn replace_rc(&mut self, id: EltId, elt: Rc<C::Element>) -> Result<Rc<C::Element>, ElementOp> {
        let class_id = if let Some(class_id) = self.classifier.classify(&*elt) {
            class_id
        } else {
            match self.classifier.fallback() {
                ClassifyFallback::Default(class_id) => class_id,
                ClassifyFallback::ReplacedOrFail | ClassifyFallback::ReplacedOrDefault(_) => id.part_id(),
                ClassifyFallback::Fail => {
                    return Err(ElementOp::ClassifyFailure);
                },
            }
        };
        if class_id != id.part_id() {
            // Different partition; we need to move.
            // 1: Confirm we have the source partition available or abort.
            let source_id = id.part_id();
            try!(if let Some(mut _source_state) = self.states.get_mut(&source_id) {
                // TODO: do we want to notify that `id` is about to be moved?
                Ok(())
            } else {
                Err(ElementOp::NotLoaded)
            });
            // 2: Find target partition and insert element.
            let new_id = try!(if let Some(mut target_state) = self.states.get_mut(&class_id) {
                match target_state.insert_with_id(class_id.elt_id(id.elt_num()), elt.clone()) {
                    // success with the same element part of the id:
                    Ok(id) => Ok(id),
                    // failure; try with a new id:
                    Err(_) => target_state.insert_rc(elt)
                }
            } else {
                Err(ElementOp::NotLoaded)
            });
            // 3: Remove from source partition. We must find `source_state`
            // again because `self.states` does not support simultaneous
            // mutable references to two of its elements.
            if let Some(mut source_state) = self.states.get_mut(&source_id) {
                let removed = try!(source_state.remove(id));
                source_state.set_move(id, new_id);
                Ok(removed)
            } else {
                Err(ElementOp::NotLoaded)
            }
        } else {
            // Same partition: just replace
            if let Some(mut state) = self.states.get_mut(&class_id) {
                state.replace_rc(id, elt)
            } else {
                Err(ElementOp::NotLoaded)
            }
        }
    }
    fn remove(&mut self, id: EltId) -> Result<Rc<C::Element>, ElementOp> {
        let part_id = id.part_id();
        if let Some(mut state) = self.states.get_mut(&part_id) {
            state.remove(id)
        } else {
            Err(ElementOp::NotLoaded)
        }
    }
}
