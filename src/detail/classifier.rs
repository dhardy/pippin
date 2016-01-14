//! Classification of Pippin elements

use std::convert::From;
use std::io::Write;
use std::marker::PhantomData;

use super::ElementT;
use ::error::{Result, Error};

/// A classification / partition number
/// 
/// This number must not be zero and the high 24 bits must be zero (it must be
/// less than 2^40).
/// 
/// Create via `From`: `PartNum::from(n)`, which introduces a bounds
/// check.
/// 
/// The same numbers are used for classification as for partitions: the numbers
/// returned by `initial()` and `classify()` are also partition identifiers.
/// 
/// These numbers can never be zero. Additionally, they are restricted to 40
/// bits; the high 24 bits must be zero. Element identifiers are take the form
/// `(part_num << 24) + gen_id()` where `part_num` is the partition number and
/// `gen_id()` returns a 24-bit number unique within the partition.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct PartNum {
    // #0018: optimise usage as Option with NonZero?
    num: u64,
}
impl PartNum {
    /// Returns a non-zero number whose low 24 bits are all zero.
    pub fn as_id(self) -> u64 { self.num << 24 }
}
impl From<u64> for PartNum {
    fn from(n: u64) -> PartNum {
        assert!(n != 0 && n < (1<<40), "check bounds on classification / partition number");
        PartNum {
            num: n
        }
    }
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
    
    /// Initially there should only be one partition and one classification.
    /// This function gets the number of this classification.
    /// 
    /// The return value must not be zero (see `ClassifierT` documentation on
    /// numbers). One is a perfectly decent initial value.
    /// 
    /// It is allowed for this function to panic once there is more than one
    /// classification available.
    fn initial(&self) -> PartNum;
    
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
    /// The function should return the numbers of the new classifications.
    /// 
    /// If this fails with `ReclassifyError::OutOfNumbers`, the "number
    /// stealing" logic will be activated (see `steal_target()` and
    /// `steal_from()`). This allows numbers to be redistributed, and on
    /// success, `divide()` will be called again.
    fn divide(&mut self, class: PartNum) ->
        Result<Vec<PartNum>, ReclassifyError>;
    
    /// In case `ReclassifyError::OutOfNumbers` is returned, this function is
    /// called. It is used to suggest a partition to steal numbers from.
    /// 
    /// It is an error if `divide` returns `Err(ReclassifyError::OutOfNumbers)`
    /// *and* this function returns `None`. The default implementation returns
    /// `None`, on the assumption that many implementations will not need this
    /// logic.
    fn steal_target(&self) -> Option<PartNum> { None }
    
    /// After `steal_target()` is called, the partition in question is loaded,
    /// the classifier updated, then this function called.
    /// 
    /// This should attempt to steal numbers from the target for the second
    /// classification, to allow its subdivision. On success, it should update
    /// internal information and return the numbers of the partitions which
    /// need to have their headers updated with new info (as supplied by
    /// `write`). On failure, its internal state should be updated such that
    /// `steal_target` returns a new number.
    /// 
    /// The default implementation returns `Err(StealError::GiveUp)`.
    fn steal_from(&mut self, _target_id: PartNum,
        _for_id: PartNum) -> Result<Vec<PartNum>, StealError>
    {
        Err(StealError::GiveUp)
    }
    
    /// This function lets a classifier write out whatever it knows about
    /// partitions to some piece of data, stored in a partition header.
    fn write_buf(&self, writer: &mut Write) -> Result<()>;
    
    /// This function is called whenever a partition header is loaded with
    /// information about classifications. If there are multiple partitions in
    /// the repository, it may well be called multiple times at program
    /// start-up, and also later. The classifier should use per-partition
    /// versioning to decide which information is more up-to-date than the
    /// currently stored information.
    fn read_buf(&mut self, buf: &[u8]) -> Result<()>;
}

/// Failures allowed for `ClassifierT::divide`.
pub enum ReclassifyError {
    /// No logic is available allowing subdivision of the category.
    NotSubdivisible,
    /// Activates "number stealing" logic.
    OutOfNumbers,
    /// Any other error.
    Other(Error),
}

/// Failures allowed for `ClassifierT::steal_from`.
pub enum StealError {
    /// Switch to another target.
    SwitchTarget,
    /// Give up. Really not ideal if it comes to this.
    GiveUp,
    /// Any other error (assumed to be temporary).
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
}
impl<E: ElementT> ClassifierT for DummyClassifier<E> {
    type Element = E;
    fn initial(&self) -> PartNum { PartNum::from(1) }
    fn classify(&self, _elt: &Self::Element) -> Option<PartNum> {
        Some(PartNum::from(1))
    }
    fn divide(&mut self, _class: PartNum) ->
        Result<Vec<PartNum>, ReclassifyError>
    {
        Err(ReclassifyError::NotSubdivisible)
    }
    fn write_buf(&self, _writer: &mut Write) -> Result<()> { Ok(()) }
    fn read_buf(&mut self, _buf: &[u8]) -> Result<()> { Ok(()) }
}
