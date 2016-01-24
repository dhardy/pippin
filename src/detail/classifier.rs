//! Classification of Pippin elements

use std::convert::From;
use std::io::Write;
use std::marker::PhantomData;
use std::any::Any;

use super::{ElementT, PartNum};
use super::{PartitionIO};
use ::error::{Result, Error};

/// Provides file discovery and creation for a repository.
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
    fn partitions(&self) -> Vec<PartNum>;
    
    /// Add a new partition. `num` is the partition number to use; this function
    /// fails if it is already taken. `prefix` is a relative path plus file-name
    /// prefix, e.g. `data/misc-` would result in a snapshot having a name like
    /// `misc-pn1-ss1.pip` inside the `data` subdirectory.
    /// 
    /// On success, returns the index of the new partition (for use with
    /// `make_partition_io()`).
    fn add_partition(&mut self, num: PartNum, prefix: &str) -> Result<()>;
    
    /// Construct and return a new PartitionIO for partition `num`.
    /// 
    /// Fails if construction of the PartitionIO fails (file-system or regex
    /// errors) or if the partition isn't found.
    fn make_partition_io(&self, num: PartNum) -> Result<Box<PartitionIO>>;
}


/// A classifier is a device taking an element and returning a numeric code
/// classifying that element. See notes on partitioning and classification.
/// 
/// The user must supply an implementation of this trait in order to use the
/// `Repo` type (repository). The user-defined *element* type must be specified
/// within objects implementing this trait in order to tie the two
/// user-specified types together.
/// 
/// Implementations must provide at least `Element`, `initial`, `classify`,
/// `divide`, `read_buf` and `write_buf`.
pub trait ClassifierT {
    /// The user-specified element type.
    type Element: ElementT;
    
    /// Get access to the I/O provider. This could be an instance of
    /// `DiscoverRepoFiles` or could be self (among other possibilities).
    fn repo_io<'a>(&'a mut self) -> &'a mut RepoIO;
    
    /// Initially there should only be one partition and one classification.
    /// This function returns the number of this classification and a
    /// `PartitionIO`.
    /// 
    /// The return value must not be zero (see `ClassifierT` documentation on
    /// numbers). One is a perfectly decent initial value.
    /// 
    /// It is allowed for this function to panic once there is more than one
    /// classification available.
    /// 
    /// The default implementation uses number 1 and calls `add_partition` and
    /// `make_partition_io`.
    fn first_part(&mut self) -> Result<(PartNum, Box<PartitionIO>)> {
        let num = PartNum::from(1);
        let mut io = self.repo_io();
        try!(io.add_partition(num, "" /*no prefix*/));
        let part_io = try!(io.make_partition_io(num));
        Ok((num, part_io))
    }
    
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
    fn classify(&self, elt: &Self::Element) -> Option<PartNum>;
    
    /// This is used only when `classify` returns `None` for an element.
    /// 
    /// This is only needed for cases where some operations should be supported
    /// despite classification not being available in all cases. The default
    /// implementation returns `ClassifyFallback::Fail`.
    fn fallback(&self) -> ClassifyFallback { ClassifyFallback::Fail }
    
    /// This function is called when too many elements correspond to the given
    /// classification. The function should divide this classification into two
    /// or more new classifications with new numbers; the number of the old
    /// classification should not be used again (unless somehow the new
    /// classifications were to recombined into the old).
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
    /// `DivideError::LoadPart(num)`.
    fn divide(&mut self, class: PartNum) ->
        Result<(Vec<PartNum>, Vec<PartNum>), DivideError>;
    
    // #0025: provide a choice of how to implement IO via a const bool?
    
    /// This function lets a classifier write out whatever it knows about
    /// partitions to some piece of data, stored in a partition header.
    /// 
    /// The `num` indicates which partition this will be stored in.
    fn write_buf(&self, num: PartNum, writer: &mut Write) -> Result<()>;
    
    /// This function is called whenever a partition header is loaded with
    /// information about classifications. If there are multiple partitions in
    /// the repository, it may well be called multiple times at program
    /// start-up, and also later. The classifier should use per-partition
    /// versioning to decide which information is more up-to-date than the
    /// currently stored information.
    /// 
    /// The `num` indicates which partition this was stored in.
    fn read_buf(&mut self, num: PartNum, buf: &[u8]) -> Result<()>;
}

/// Failures allowed for `ClassifierT::divide`.
pub enum DivideError {
    /// No logic is available allowing subdivision of the category.
    NotSubdivisible,
    /// Used when another partition needs to be loaded before division, e.g.
    /// to steal allocated numbers.
    LoadPart(PartNum),
    /// Any other error.
    Other(Error),
}

/// Specifies what to do when classification fails and an element is to be
/// inserted or replaced.
pub enum ClassifyFallback {
    /// Use the given classification for an insertion or replacement.
    Default(PartNum),
    /// In the case of a replacement, assume the replacing element has the
    /// same classification as the element being replaced. If not a
    /// replacement, use the default specified.
    ReplacedOrDefault(PartNum),
    /// In the case of a replacement, assume the replacing element has the
    /// same classification as the element being replaced. If not a
    /// replacement, fail.
    ReplacedOrFail,
    /// Fail the operation. The insertion or replacement operation will fail
    /// with an error.
    Fail,
}

/// Trivial implementation for testing purposes. Always returns the same value,
/// 1, thus there will only ever be a single 'partition'.
pub struct DummyClassifier<E: ElementT> {
    p: PhantomData<E>,
    io: Box<RepoIO>,
}
impl<E: ElementT> DummyClassifier<E> {
    /// Create an instance owning a boxed `RepoIO`
    pub fn new(io: Box<RepoIO>) -> DummyClassifier<E> {
        DummyClassifier { p: PhantomData, io: io }
    }
}
impl<E: ElementT> ClassifierT for DummyClassifier<E> {
    type Element = E;
    fn repo_io<'a>(&'a mut self) -> &'a mut RepoIO {
        &mut *self.io
    }
    fn classify(&self, _elt: &Self::Element) -> Option<PartNum> {
        Some(PartNum::from(1))
    }
    fn divide(&mut self, _class: PartNum) ->
        Result<(Vec<PartNum>, Vec<PartNum>), DivideError>
    {
        Err(DivideError::NotSubdivisible)
    }
    fn write_buf(&self, _num: PartNum, _writer: &mut Write) -> Result<()> { Ok(()) }
    fn read_buf(&mut self, _num: PartNum, _buf: &[u8]) -> Result<()> { Ok(()) }
}
