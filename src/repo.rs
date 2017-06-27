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
//! *   `Classify` to classify elements, along with an `Element` type
//! *   `RepoControl`. This type should handle partitioning, creation of `Classify`
//!     objects, saving and discovering partitioning information, and provide
//!     the `RepoIO` implementation

use std::result;
use std::collections::hash_map::{HashMap, Values, ValuesMut};
use std::rc::Rc;
use std::mem::swap;

use classify::{PropId, Property, CsfFinder};
use elt::{EltId, PartId, Element};
use error::{ClassifyError, Result, OtherError, TipError, ElementOp, RepoDivideError};
use io::RepoIO;
use part::{Partition, PartControl};
use merge::TwoWaySolver;
use state::{StateRead, StateWrite, MutPartState};


/// User-defined controls on repository operation.
pub trait RepoControl {
    /// User-defined type of elements stored
    type Element: Element;
    
    /// Type implementing `part::PartControl`
    type PartControl: PartControl<Element = Self::Element>;
    
    /// Get access to the I/O provider. This could be an instance of
    /// `DiscoverRepoFiles` or could be self (among other possibilities).
    fn io(&self) -> &RepoIO;
    
    /// Get mutable access to the I/O provider. This could be an instance of
    /// `DiscoverRepoFiles` or could be self (among other possibilities).
    fn io_mut(&mut self) -> &mut RepoIO;
    
    /// Get a `PartControl` object for existing partition `num`.
    fn make_part_control(&mut self, num: PartId) -> Result<Self::PartControl>;
    
    /// Get a property function by identifier, if available.
    /// 
    /// TODO: how should missing functions be handled?
    fn prop_fn(&self, id: PropId) -> Option<Property<Self::Element>>;
    
    /// This method is called once by `Repository::create()`. It can do any initialisation required
    /// and should return a partition number for the first partition.
    /// 
    /// The default implementation simply returns `Ok(PartId::from_num(1))`.
    fn init_first(&mut self) -> Result<PartId> {
        Ok(PartId::from_num(1))
    }
    
    /// Allows users to pick human-readable prefixes for partition file names.
    /// The default implementation returns `None`.
    /// 
    /// If `None` is returned, the library uses `format!("pn{}", part_id)`.
    /// Otherwise, it is suggested but not required that the partition number
    /// feature in this prefix (the only requirement is uniqueness).
    fn suggest_part_prefix(&mut self, _part_id: PartId) -> Option<String> {
        None
    }
    
    /// Determines whether a partition should be divided.
    /// 
    /// This is called by `Repository::write_all()` on all partitions.
    /// 
    /// The default implementation returns `false` (never divide). A simple
    /// working version could base its decision on the number of elements
    /// contained, e.g.
    /// `part.tip().map_or(false, |state| state.num_avail()) > 10_000`.
    fn should_divide(&mut self, _part_id: PartId, _part: &Partition<Self::PartControl>)
            -> bool
    {
        false
    }
    
    /// This function is called when too many elements correspond to the given
    /// classification (see `should_divide()`). The function should create new
    /// partition numbers and update the classifier to reassign some or all
    /// elements of the existing partition. Elements are moved only from the
    /// source ("divided") partition, and can be moved to any partition.
    /// 
    /// The divided partition cannot be destroyed or its number
    /// reassigned, but it can still have elements assigned.
    /// 
    /// The return value should be `Ok((new_ids, changed))` on success where
    /// `new_ids` are the partition numbers of new partitions (to be created)
    /// and `changed` are the numbers of partitions whose `UserFields` must be
    /// updated (via a new snapshot or change log). Normally `changed` may be
    /// empty, but this strategy allows assigning and "stealing" ranges of free
    /// partition numbers.
    /// 
    /// This may fail with `RepoDivideError::NotSubdivisible` if the partition
    /// cannot be divided at this time. It may fail with
    /// `RepoDivideError::LoadPart(num)`; this causes the numbered partition to
    /// be loaded then this function called again (may be useful for "stealing"
    /// partition numbers). Any other error will cause the operation doing the
    /// division to fail.
    /// 
    /// After division, a special strategy is used to move elements safely:
    /// 
    /// 1.  the divided partition is saved with a special code noting that
    ///     elements are being moved
    /// 2.  new partitions are created (TODO: what if this fails?)
    /// 3.  "changed" partitions are saved
    /// 4.  a table is made listing where elements of the divided partition
    ///     should go, then for each target partition elements are inserted,
    ///     the partition saved, then the elements are removed from the divided
    ///     partition and this saved (TODO: in multiple stages if large number?
    ///     how to avoid duplication on failure?)
    /// 5.  a new snapshot is written for the divided partition
    /// 
    /// Details of the new partitioning may be stored in the `UserFields` of
    /// each partition which gets touched. This may not be all partitions, so
    /// code handling loading of `UserFields` needs to use per-partition
    /// versioning to determine which information is up-to-date.
    /// 
    /// The default implementation returns `Err(RepoDivideError::NotSubdivisible)`.
    fn divide(&mut self, _part: &Partition<Self::PartControl>) ->
            Result<(Vec<PartId>, Vec<PartId>), RepoDivideError> {
        Err(RepoDivideError::NotSubdivisible)
    }
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
pub struct Repository<R: RepoControl> {
    /// Classifier. This must use compile-time polymorphism since it gives us
    /// the element type, and we do not want element look-ups to involve a
    /// run-time conversion.
    control: R,
    /// Descriptive identifier for the repository
    name: String,
    /// List of loaded partitions, by their `PartId`.
    partitions: HashMap<PartId, Partition<R::PartControl>>,
    /// Classification finder
    csf_finder: CsfFinder<R::Element>,
}

// Non-member functions on Repository
impl<R: RepoControl> Repository<R> {
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
    pub fn create<S: Into<String>>(mut control: R, name: S) -> Result<Repository<R>>
    {
        let name: String = name.into();
        info!("Creating repository: {}", name);
        
        let part_id = control.init_first()?;
        let suggestion = control.suggest_part_prefix(part_id);
        let prefix = suggestion.unwrap_or_else(|| format!("pn{}", part_id));
        
        control.io_mut().new_part(part_id, prefix)?;
        let part_control = control.make_part_control(part_id)?;
        let part = Partition::create(part_id, part_control, &name)?;
        
        let mut csf_finder = CsfFinder::new();
        csf_finder.add_csf(part_id, part.csf(), &control)?;
        
        let mut partitions = HashMap::new();
        partitions.insert(part.part_id(), part);
        
        Ok(Repository{
            control,
            name,
            partitions,
            csf_finder,
        })
    }
    
    /// Open an existing repository for read/write.
    /// 
    /// If `read_data` is true, the latest state of each partition is read into memory immediately.
    /// Otherwise, initialise each partition without reading data (currently requires reading a
    /// snapshot header).
    pub fn open(mut control: R, read_data: bool)-> Result<Repository<R>> {
        let mut name = None;
        let mut csf_finder = CsfFinder::new();
        let mut partitions = HashMap::new();
        for part_id in control.io().parts() {
            let part_control = control.make_part_control(part_id)?;
            let part = Partition::open(part_id, part_control, read_data)?;
            // #0061: lifetime analysis sucks, or Option needs an Entry API
            let has_name = if let Some(repo_name) = name.as_ref() {
                if part.repo_name() != repo_name {
                    return OtherError::err("repository name does not match when loading (wrong repo?)");
                }
                true
            } else {
                false
            };
            if !has_name {
                name = Some(part.repo_name().to_string());
            }
            csf_finder.add_csf(part_id, part.csf(), &control)?;
            partitions.insert(part_id, part);
        }
        
        let name = if let Some(repo_name) = name {
            repo_name
        } else {
            return OtherError::err("No repository files found");
        };
        
        info!("Successfully opened repository with {} partitions: {}", partitions.len(), name);
        Ok(Repository{
            control,
            name,
            partitions,
            csf_finder,
        })
    }
}

// Member functions on Repository — a set of elements.
impl<R: RepoControl> Repository<R> {
    /// Get the repo name
    pub fn name(&self) -> &str { &self.name }
    
    /// Iterate over all partitions.
    /// 
    /// These do not necessarily have data loaded; use `load_latest()`
    /// or one of the `Partition::load_...()` operations.
    pub fn partitions(&self) -> PartIter<R::PartControl> {
        PartIter { iter: self.partitions.values() }
    }
    
    /// Get a mutable iterator over partitions.
    /// 
    /// These do not necessarily have data loaded; use `load_latest()`
    /// or one of the `Partition::load_...()` operations.
    pub fn partitions_mut(&mut self) -> PartIterMut<R::PartControl> {
        PartIterMut { iter: self.partitions.values_mut() }
    }
    
    /// Load the latest state of all partitions
    pub fn load_latest(&mut self) -> Result<()> {
        for part in &mut self.partitions.values_mut() {
            part.load_latest()?;
        }
        Ok(())
    }
    
    /// Write commits to the disk for all partitions.
    /// 
    /// Also see the `write_full()` function.
    pub fn write_fast(&mut self) -> Result<()> {
        for part in &mut self.partitions.values_mut() {
            part.write_fast()?;
        }
        Ok(())
    }
    /// Write commits to the disk for all partitions and do any needed
    /// maintenance operations.
    /// 
    /// This should be called at least occasionally, but such calls could be
    /// scheduled during less busy periods.
    pub fn write_full(&mut self) -> Result<()> {
        // Write all logs first, in case we crash later
        self.write_fast()?;
        for part in &mut self.partitions.values_mut() {
            part.write_full()?;
        }
        
        // Maintenance: this is where repartitioning happens. Two steps are involved:
        // (1) creating the new classifications and (2) moving elements from the old partition.
        // TODO: create new partitions first? Delete old partition afterwards?
        
        // This is tricky due to lifetime analysis preventing re-use of partitions. So,
        // 1) we collect partition numbers of any partition needing repartitioning.
        let mut should_divide: Vec<PartId> = Vec::new();
        let mut need_reclassify: Vec<PartId> = Vec::new();
        for (id, part) in &self.partitions {
            if self.control.should_divide(*id, part) && part.is_ready() {
                should_divide.push(*id);
            }
            if let Ok(state) = part.tip() {
                if state.meta().ext_flags().flag_reclassify() {
                    need_reclassify.push(*id);
                }
            }
        }
        
        while let Some(old_id) = should_divide.pop() {
            // Get new partition numbers and update classifiers. This gets saved later if successful.
            let result = self.control.divide(self.partitions.get(&old_id).expect("get partition"));
            let (new_parts, changed) = match result {
                Ok(result) => result,
                Err(RepoDivideError::NotSubdivisible) => {
                    continue;
                },
                Err(RepoDivideError::LoadPart(pid)) => {
                    if let Some(mut part) = self.partitions.get_mut(&pid) {
                        part.load_latest()?;
                        should_divide.push(old_id); // try again
                        continue;
                    } else {
                        error!("Division requested load of partition {}, but partition was not found", pid);
                        return OtherError::err("requested partition not found during division");
                    }
                },
                Err(e) => {
                    return Err(Box::new(e));
                }
            };
            
            // Mark partition as needing reclassification:
            {
                let old_part = self.partitions.get_mut(&old_id).expect("has old part");
                let mut tip = old_part.tip()?.clone_mut();
                tip.meta_mut().ext_flags_mut().set_flag_reclassify(true);
                old_part.push_state(tip)?;
                old_part.write_fast()?;
                if !need_reclassify.contains(&old_id) {
                    need_reclassify.push(old_id);
                }
            }
            
            // Create new partitions:
            for new_id in new_parts {
                let suggestion = self.control.suggest_part_prefix(new_id);
                let prefix = suggestion.unwrap_or_else(|| format!("pn{}", new_id));
                self.control.io_mut().new_part(new_id, prefix)?;
                let part_control = self.control.make_part_control(new_id)?;
                let mut part = Partition::create(new_id, part_control, &self.name)?;
                part.write_full()?;
                self.partitions.insert(new_id, part);
            }
            
            // Save all changed partitions:
            for id in changed {
                match self.partitions.get_mut(&id) {
                    Some(part) => {
                        //TODO: snapshot or log?
                        //TODO: continue on fail (i.e. require write later)?
                        part.write_snapshot()?;
                    },
                    None => {
                        warn!("Was notified that partition {} changed, but couldn't find it!", id);
                    },
                }
            }
        }
        
        while let Some(old_id) = need_reclassify.pop() {
            // extract from partitions so as not to block it
            let mut old_part = self.partitions.remove(&old_id).expect("remove old part");
            
            // Check all elements and record each that needs moving (where, list of elements):
            let mut target_part_elts = HashMap::<PartId, Vec<EltId>>::new();
            for (elt_id, elt) in old_part.tip()?.elts_iter() {
                // TODO: find_part_id_for_elt will probably be expensive; would it be better to
                // check each element against each possible destination? Possibly not if
                // classifiers are slow?
                if let Ok(part_id) = self.find_part_id_for_elt(&*elt) {
                    target_part_elts.entry(part_id).or_insert(vec![])
                            .push(elt_id);
                } // else: don't move anything we can't reclassify
                // TODO: if we can't classify an element, should we still remove the reclassify
                // flag below?
            }
            
            // For each destination, move elements needing to go there:
            for (part_id, old_elt_ids) in target_part_elts {
                let mut part = match self.partitions.get_mut(&part_id) {
                    Some(p) => p,
                    None => {
                        // TODO: skip, load, ...?
                        unimplemented!();
                    }
                };
                let mut state = part.tip()?.clone_mut();
                let mut old_state = old_part.tip()?.clone_mut();
                for elt_id in old_elt_ids {
                    // TODO: if there are a lot of elements/data, we should stop and write a
                    // checkpoint from time to time.
                    if let Ok(elt) = old_state.remove(elt_id) {
                        let id = state.free_id_near(elt_id.elt_num())?;
                        state.insert_rc(id, elt)?;
                    }
                }
                part.push_state(state)?;
                part.write_full()?;
                
                // Do a fast write now to save removals:
                old_part.push_state(old_state)?;
                old_part.write_fast()?;
            }
            
            // Finally, remove the 'reclassify' flag on the old partition, write a snapshot and
            // re-insert it:
            let mut tip = old_part.tip()?.clone_mut();
            tip.meta_mut().ext_flags_mut().set_flag_reclassify(false);
            old_part.push_state(tip)?;
            old_part.require_snapshot();
            old_part.write_full()?;
            self.partitions.insert(old_id, old_part);
        }
        
        Ok(())
    }
    
    /// Force all loaded partitions to write a snapshot.
    pub fn write_snapshot_all(&mut self) -> Result<()> {
        for part in &mut self.partitions.values_mut() {
            part.write_snapshot()?;
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
        for part in &mut self.partitions.values_mut() {
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
    pub fn merge<S: TwoWaySolver<R::Element>>(&mut self, solver: &S, auto_load: bool) -> Result<()>
    {
        for part in &mut self.partitions.values_mut() {
            part.merge(solver, auto_load)?;
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
    pub fn clone_state(&self) -> result::Result<RepoState<R::Element>, TipError> {
        let mut rs = RepoState::new(self.csf_finder.clone());
        for (num, part) in &self.partitions {
            if part.is_loaded() {
                rs.add_part(*num, part.tip()?.clone_mut());
            }
        }
        Ok(rs)
    }
    
    /// Merge changes from a `RepoState` into the repo, consuming the
    /// `RepoState`.
    /// 
    /// Returns true when any further merge work is required. In this case
    /// `merge()` should be called.
    pub fn merge_in(&mut self, state: RepoState<R::Element>) -> Result<bool> {
        let mut merge_required = false;
        for (num, pstate) in state.states {
            let mut part = if let Some(p) = self.partitions.get_mut(&num) {
                p
            } else {
                panic!("RepoState has a partition not found in the Repository");
                //TODO: support for merging after a division/union/change of partitioning
            };
            let is_new = part.push_state(pstate)?;
            if is_new && part.merge_required() { merge_required = true; }
        }
        Ok(merge_required)
    }
    
    /// Merge changes from a `RepoState` and update it to the latest state of
    /// the `Repository`.
    /// 
    /// Returns true if further merge work is required. In this case, `merge()`
    /// should be called on the `Repository`, then `sync()` again (until then, the
    /// `RepoState` will have no access to any partitions with conflicts).
    pub fn sync(&mut self, state: &mut RepoState<R::Element>) -> Result<bool> {
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
            if part.push_state(pstate)? {
                if part.merge_required() {
                    merge_required = true;
                } else {
                    state.add_part(num, part.tip()?.clone_mut());
                }
            }
        }
        
        for (num, part) in &self.partitions {
            if !state.has_part(*num) {
                state.add_part(*num, part.tip()?.clone_mut());
            }
        }
        Ok(merge_required)
    }
    
    // TODO: should this be on a repostate?
    fn find_part_id_for_elt(&self, elt: &R::Element) -> Result<PartId, ClassifyError> {
        // TODO: improve algorithm. Cache classifier outputs locally? Search by partition instead
        // of by element? Or search by element then by classifier, using some type of tree to
        // narrow down the result?
        // TODO: should we check anywhere that partition classifications don't overlap?
        for (part_id, part) in &self.partitions {
            if part.csf().matches_elt(elt, &self.control)? {
                return Ok(*part_id);
            }
        }
        Err(ClassifyError::NoPartMatches)
    }
}

/// Provides read-write access to some or all partitions in a non-blocking
/// fashion. This does not know about any partitions not internally available,
/// has no access to historical states and is not able to load more
/// data on demand.
/// 
/// This should be merged back in to the repo in order to record and
/// synchronise edits.
pub struct RepoState<E: Element> {
    states: HashMap<PartId, MutPartState<E>>,
    csf_finder: CsfFinder<E>,
}

impl<E: Element> RepoState<E> {
    /// Create new, with no partition states (use `add_part()`)
    // TODO: should we take a copy of the finder? Or a reference to Repo or to a boxed finder?
    fn new(csf_finder: CsfFinder<E>) -> RepoState<E> {
        RepoState { csf_finder: csf_finder, states: HashMap::new() }
    }
    
    /// Add a state from some partition
    fn add_part(&mut self, num: PartId, state: MutPartState<E>) {
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
    
    /// TODO: this is used by seq_create_small but is not a good API
    pub fn insert_near(&mut self, initial: u32, elt: E) -> Result<EltId, ElementOp> {
        if let Some(part_id) = self.csf_finder.find(&elt) {
            if let Some(mut state) = self.states.get_mut(&part_id) {
                let id = state.free_id_near(initial)?;
                return state.insert(id, elt);
            }
        }
        // In this case no classification matched; probably we just need to load a partition?
        // TODO: try to find & load the partition? Change error code?
        Err(ElementOp::PartNotFound)
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
                return Err(ElementOp::PartNotFound);
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
        Err(ElementOp::EltNotFound)
    }
}

impl<E: Element> StateRead<E> for RepoState<E> {
    fn any_avail(&self) -> bool {
        self.states.values().any(|v| v.any_avail())
    }
    fn num_avail(&self) -> usize {
        self.states.values().fold(0, |acc, v| acc + v.num_avail())
    }
    fn is_avail(&self, id: EltId) -> bool {
        let part_id = id.part_id();
        self.states.get(&part_id).map_or(false, |state| state.is_avail(id))
    }
    fn get_rc(&self, id: EltId) -> Result<&Rc<E>, ElementOp> {
        let part_id = id.part_id();
        match self.states.get(&part_id) {
            Some(state) => state.get_rc(id),
            None => Err(ElementOp::PartNotFound),
        }
    }
}
impl<E: Element> StateWrite<E> for RepoState<E> {
    fn insert_rc(&mut self, id: EltId, elt: Rc<E>) -> Result<EltId, ElementOp> {
        // TODO: verify classification?
        if let Some(mut state) = self.states.get_mut(&id.part_id()) {
            state.insert_rc(id, elt)
        } else {
            // TODO: try to find & load the partition?
            Err(ElementOp::PartNotFound)
        }
    }
    
    fn insert_new_rc(&mut self, elt: Rc<E>) -> Result<EltId, ElementOp> {
        if let Some(part_id) = self.csf_finder.find(&*elt) {
            if let Some(mut state) = self.states.get_mut(&part_id) {
                return state.insert_new_rc(elt);
            }
        }
        // In this case no classification matched; probably we just need to load a partition?
        // TODO: try to find & load the partition? Change error code?
        Err(ElementOp::PartNotFound)
    }
    
    fn replace_rc(&mut self, id: EltId, elt: Rc<E>) -> Result<Rc<E>, ElementOp> {
        // TODO: verify classification?
        if let Some(mut state) = self.states.get_mut(&id.part_id()) {
            state.replace_rc(id, elt)
        } else {
            // TODO: try to find & load the partition?
            Err(ElementOp::PartNotFound)
        }
    }
    
    fn remove(&mut self, id: EltId) -> Result<Rc<E>, ElementOp> {
        if let Some(mut state) = self.states.get_mut(&id.part_id()) {
            state.remove(id)
        } else {
            Err(ElementOp::PartNotFound)
        }
    }
}

/// Iterator over partitions.
pub struct PartIter<'a, P: PartControl+'a> {
    iter: Values<'a, PartId, Partition<P>>
}
impl<'a, P: PartControl> Iterator for PartIter<'a, P> {
    type Item = &'a Partition<P>;
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
    fn size_hint(&self) -> (usize, Option<usize>) { self.iter.size_hint() }
}

/// Mutating iterator over partitions.
pub struct PartIterMut<'a, P: PartControl+'a> {
    iter: ValuesMut<'a, PartId, Partition<P>>
}
impl<'a, P: PartControl> Iterator for PartIterMut<'a, P> {
    type Item = &'a mut Partition<P>;
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
    fn size_hint(&self) -> (usize, Option<usize>) { self.iter.size_hint() }
}
