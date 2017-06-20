/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Traits for Pippin's `Repository` type

use classify::{PropId, Property};
use elt::{Element, PartId};
use error::{Result, RepoDivideError};
use io::RepoIO;
use part::{Partition, PartControl};


/// Encapsulates a `RepoIO` and a `Classify`, handling repartitioning and
/// serialisation.
/// 
/// Implementations should also implement `UserFields` to store and retrieve
/// metadata from file headers; e.g. if partitioning is not fixed, the
/// classifier will have mutable state which needs to be written and
/// reconstructed. It is up to the user to implement versioning for this data
/// so that the latest version can be reconstructed.
/// 
/// It is recommended that information stored on partitions and partitioning
/// is versioned independently for *each partition* so that when metadata is
/// recovered from headers, a correct version is built even if multiple
/// partitions had been modified independently.
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
    fn prop_fn(&self, id: PropId) -> Option<&Property<Element = Self::Element>>;
    
    /// This method is called once by `Repository::create()`. It should
    /// initialise the classifier for a new repository (if the classifier
    /// requires this) and return an identifier for the first partition.
    /// 
    /// If no initialisation is needed, this may simply return a PartId:
    /// 
    /// ```no_compile
    /// fn init_first(&mut self) -> Result<PartId> {
    ///     Ok(PartId::from_num(1))
    /// }
    /// ```
    /// 
    /// It is allowed for this function to panic if it is called a second time
    /// or after any method besides `io()` has been called.
    fn init_first(&mut self) -> Result<PartId>;
    
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
    fn divide(&mut self, part: &Partition<Self::PartControl>) ->
        Result<(Vec<PartId>, Vec<PartId>), RepoDivideError>;
}
