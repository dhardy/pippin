/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Traits for Pippin's `Repository` type

use std::marker::PhantomData;
use std::any::Any;
use std::io::Write;

use PartIO;
use {ElementT, PartId, PartState};
use error::{Error, Result, OtherError};


/// Provides file discovery and creation for a repository.
//TODO: use 'part' instead of 'partition' in names?
pub trait RepoIO {
    /// Convert self to a `&Any`
    fn as_any(&self) -> &Any;
    
    /// Get the number of partitions found.
    fn num_partitions(&self) -> usize;
    
    /// Get a list of all partition numbers. These are the numbers which can be
    /// passed to `make_partition_io`, and conversely the numbers which should
    /// not be passed to `add_partition`.
    /// 
    /// Note: we cannot 'simply iterate' over elements without allocating
    /// unless we make more restrictions on implementations or switch to
    /// compile-time polymorphism over type `RepoIO`.
    fn partitions(&self) -> Vec<PartId>;
    
    /// True if there is a partition with this number
    fn has_partition(&self, pn: PartId) -> bool;
    
    /// Add a new partition. `num` is the partition number to use; this function
    /// fails if it is already taken. `prefix` is a relative path plus file-name
    /// prefix, e.g. `data/misc-` would result in a snapshot having a name like
    /// `misc-pn1-ss1.pip` inside the `data` subdirectory.
    //TODO: should this be `new_part`?
    fn add_partition(&mut self, num: PartId, prefix: &str) -> Result<()>;
    
    /// Construct and return a new PartIO for partition `num`.
    /// 
    /// Fails if construction of the PartIO fails (file-system or regex
    /// errors) or if the partition isn't found.
    fn make_partition_io(&self, num: PartId) -> Result<Box<PartIO>>;
}

/// A classifier is a device taking an element and returning a numeric code
/// classifying that element. See notes on partitioning and classification.
/// 
/// The user must supply an implementation of this trait in order to use the
/// `Repository` type (repository). The user-defined *element* type must be specified
/// within objects implementing this trait in order to tie the two
/// user-specified types together.
/// 
/// Implementations must provide at least `Element`, `initial`, `classify`,
/// `divide`, `read_buf` and `write_buf`.
/// 
/// Implementations must also be clonable, but clones do not need to support
/// I/O (only `Element`, `classify()` and `fallback()` must be implemented).
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
pub trait RepoT<C: ClassifierT+Sized> {
    /// Get access to the I/O provider. This could be an instance of
    /// `DiscoverRepoFiles` or could be self (among other possibilities).
    fn repo_io<'a>(&'a mut self) -> &'a mut RepoIO;
    
    /// Make a copy of the classifier. This should be independent (for use with
    /// `Repository::clone_state()`) and be unaffected by repartitioning (e.g.
    /// `divide()`) of this object. Assuming this object is not repartitioned,
    /// both self and the returned object should return the same
    /// classifications.
    fn clone_classifier(&self) -> C;
    
    /// This method is called once by `Repository::create()`. It may be used
    /// initialise classification for a new repository. It must choose an
    /// initial `PartId`, create a partition, and return its `PartIO`.
    /// 
    /// Sample code to do this (apart from any internal set-up required):
    /// 
    /// ```no_compile
    /// fn init_first(&mut self) -> Result<Box<PartIO>> {
    ///     let part_id = PartId::from_num(1);
    ///     try!(self.repo_io().add_partition(part_id, "" /*no prefix*/));
    ///     Ok(try!(self.repo_io().make_partition_io(part_id)))
    /// }
    /// ```
    /// 
    /// It is allowed for this function to panic if it is called a second time
    /// or after any method besides `repo_io()` has been called.
    fn init_first(&mut self) -> Result<Box<PartIO>>;
    
    /// This function is called when too many elements correspond to the given
    /// classification. The function should divide some partition with two
    /// or more new classifications each with new partition numbers;
    /// the number of the old classification should not be used again (unless
    /// somehow the new classifications were to recombined into the old).
    /// 
    /// The function should return the numbers of the new classifications,
    /// along with a list of other modified partitions (see below; if other
    /// partitions are not modified this should be empty).
    /// 
    /// It is possible for this function to modify other partitions, e.g. to
    /// steal numbers allocated to a different partition. In this case the
    /// second list in the result should indicate which partitions have been
    /// changed and need to be updated (a new snapshot will be created for
    /// each, which will call `write_buf(...)` in the process). In case another
    /// partition needs to be loaded first, this function may fail with
    /// `RepoDivideError::LoadPart(num)`.
    fn divide(&mut self, part: &PartState<C::Element>) ->
        Result<(Vec<PartId>, Vec<PartId>), RepoDivideError>;
    
    // #0025: provide a choice of how to implement IO via a const bool?
    
    /// This function lets a classifier write out whatever it knows about
    /// partitions to some piece of data, stored in a partition header.
    /// 
    /// The `num` indicates which partition this will be stored in.
    fn write_buf(&self, num: PartId, writer: &mut Write) -> Result<()>;
    
    /// This function is called whenever a partition header is loaded with
    /// information about classifications. If there are multiple partitions in
    /// the repository, it may well be called multiple times at program
    /// start-up, and also later. The classifier should use per-partition
    /// versioning to decide which information is more up-to-date than the
    /// currently stored information.
    /// 
    /// The `num` indicates which partition this was stored in.
    fn read_buf(&mut self, num: PartId, buf: &[u8]) -> Result<()>;
}

/// Failures allowed for `ClassifierT::divide`.
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
        RepoDivideError::Other(box OtherError::new(msg))
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
