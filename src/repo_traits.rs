/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Traits for Pippin's `Repository` type

use std::marker::PhantomData;
use std::any::Any;
use std::{fmt, result};

use {PartIO, UserFields};
use {ElementT, PartId, Partition};
use error::{Error, Result, OtherError, ErrorTrait};


/// Provides file discovery and creation for a repository.
pub trait RepoIO {
    /// Convert self to a `&Any`
    fn as_any(&self) -> &Any;
    
    /// Get the number of partitions found.
    fn num_parts(&self) -> usize;
    
    /// Get a list of all partition numbers. These are the numbers which can be
    /// passed to `make_partition_io`, and conversely the numbers which should
    /// not be passed to `add_partition`.
    /// 
    /// Note: we cannot 'simply iterate' over elements without allocating
    /// unless we make more restrictions on implementations or switch to
    /// compile-time polymorphism over type `RepoIO`.
    fn parts(&self) -> Vec<PartId>;
    
    /// True if there is a partition with this number
    fn has_part(&self, pn: PartId) -> bool;
    
    /// Add a new partition. `num` is the partition number to use; this function
    /// fails if it is already taken. `prefix` is the common part of the
    /// path/name of files for this partition; it must be unique from that of
    /// other partitions.
    fn new_part(&mut self, num: PartId, prefix: String) -> Result<()>;
    
    /// Construct and return a new PartIO for partition `num`.
    /// 
    /// Fails if construction of the PartIO fails (file-system or regex
    /// errors) or if the partition isn't found.
    fn make_part_io(&self, num: PartId) -> Result<Box<PartIO>>;
}

/// A classifier assigns each element to a partition. A repository may have
/// only a single partition, or it may have some fixed partitioning, or it may
/// have dynamic partitioning. This trait exposes the partitioning.
/// 
/// Expected usage is that the `RepoT` type will determine classification and
/// yield an object implementing this trait when `RepoT::clone_classifier()` is
/// called. The `RepoT` must implement `UserFields`, which it can use to store
/// and reconstruct the current partitioning.
pub trait ClassifierT {
    /// The user-specified element type.
    type Element: ElementT;
    
    /// Get the classification of an element.
    /// 
    /// If this returns `None`, the library assumes classification of the
    /// element is temporarily unavailable. In this case it might call
    /// `fallback`.
    /// 
    /// The return value must not be zero (see `ClassifierT` documentation on
    /// numbers).
    /// 
    /// This function is only called when inserting/replacing an element and
    /// when repartitioning, so it doesn't need to be super fast.
    fn classify(&self, elt: &Self::Element) -> Option<PartId>;
    
    /// This is used only when `classify` returns `None` for an element.
    /// 
    /// This is only needed for cases where some operations should be supported
    /// despite classification not being available in all cases. The default
    /// implementation returns `ClassifyFallback::Fail`.
    fn fallback(&self) -> ClassifyFallback { ClassifyFallback::Fail }
}

/// Specifies what to do when classification fails and an element is to be
/// inserted or replaced.
pub enum ClassifyFallback {
    /// Use the given classification for an insertion or replacement.
    Default(PartId),
    /// In the case of a replacement, assume the replacing element has the
    /// same classification as the element being replaced. If not a
    /// replacement, use the default specified.
    ReplacedOrDefault(PartId),
    /// In the case of a replacement, assume the replacing element has the
    /// same classification as the element being replaced. If not a
    /// replacement, fail.
    ReplacedOrFail,
    /// Fail the operation. The insertion or replacement operation will fail
    /// with an error.
    Fail,
}

/// Encapsulates a RepoIO and a ClassifierT, handling repartitioning and
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
pub trait RepoT<C: ClassifierT+Sized>: UserFields {
    /// Get access to the I/O provider. This could be an instance of
    /// `DiscoverRepoFiles` or could be self (among other possibilities).
    fn io<'a>(&'a mut self) -> &'a mut RepoIO;
    
    /// Make a copy of the classifier. This should be independent (for use with
    /// `Repository::clone_state()`) and be unaffected by repartitioning (e.g.
    /// `divide()`) of this object. Assuming this object is not repartitioned,
    /// both self and the returned object should return the same
    /// classifications.
    fn clone_classifier(&self) -> C;
    
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
    fn should_divide(&mut self, _part_id: PartId, _part: &Partition<C::Element>)
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
    fn divide(&mut self, part: &Partition<C::Element>) ->
        Result<(Vec<PartId>, Vec<PartId>), RepoDivideError>;
}

/// Failures allowed for `ClassifierT::divide`.
#[derive(Debug)]
pub enum RepoDivideError {
    /// No logic is available allowing subdivision of the category.
    NotSubdivisible,
    /// Used when another partition needs to be loaded before division, e.g.
    /// to steal allocated numbers.
    LoadPart(PartId),
    /// Any other error.
    Other(Error),
}
impl RepoDivideError {
    /// Create an `Other(box OtherError::new(msg))`; this is just a convenient
    /// way to create with an error message.
    pub fn msg(msg: &'static str) -> RepoDivideError {
        RepoDivideError::Other(Box::new(OtherError::new(msg)))
    }
}
impl ErrorTrait for RepoDivideError {
    fn description(&self) -> &str {
        match self {
            &RepoDivideError::NotSubdivisible => "divide: partition is not divisible",
            &RepoDivideError::LoadPart(_) => "divide: another partition needs loading",
            &RepoDivideError::Other(ref e) => e.description(),
        }
    }
}
impl fmt::Display for RepoDivideError {
    fn fmt(&self, f: &mut fmt::Formatter) -> result::Result<(), fmt::Error> {
        match self {
            &RepoDivideError::NotSubdivisible =>
                write!(f, "divide: partition is not divisible"),
            &RepoDivideError::LoadPart(id) =>
                write!(f, "divide: another partition, {}, needs loading", id),
            &RepoDivideError::Other(ref e) =>
                write!(f, "divide: other error: {}", e.description()),
        }
    }
}

/// Trivial implementation for testing purposes. Always returns the same value,
/// 1, thus there will only ever be a single 'partition'.
pub struct DummyClassifier<E: ElementT> {
    p: PhantomData<E>,
}
impl<E: ElementT> DummyClassifier<E> {
    /// Create an instance
    pub fn new() -> DummyClassifier<E> {
        DummyClassifier { p: PhantomData }
    }
}
impl<E: ElementT> ClassifierT for DummyClassifier<E> where DummyClassifier<E> : Clone {
    type Element = E;
    fn classify(&self, _elt: &Self::Element) -> Option<PartId> {
        Some(PartId::from_num(1))
    }
}
