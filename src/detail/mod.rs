//! In-memory representations of Pippin data

pub use self::element::{ElementT};
pub use self::states::{EltId, PartitionState, PartitionStateSumComparator};
pub use self::partition::{Partition, PartitionIO, PartitionDummyIO};
pub use self::discover::{DiscoverPartitionFiles, DiscoverRepoFiles};
pub use self::commits::{Commit, CommitQueue, LogReplay, EltChange};
pub use self::sum::Sum;
pub use self::repo::{Repo};
pub use self::classifier::{ClassifierT, RepoIO};

pub mod readwrite;
mod sum;
mod states;
mod commits;
mod partition;
mod discover;
mod element;
mod repo;
pub mod merge;
pub mod classifier;

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
#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash)]
pub struct PartNum {
    // #0018: optimise usage as Option with NonZero?
    num: u64,
}
impl PartNum {
    /// Extracts the number
    pub fn num(self) -> u64 { self.num }
    /// Returns a non-zero number whose low 24 bits are all zero.
    pub fn as_id(self) -> u64 { self.num << 24 }
    /// Creates from an "id" (this is the inverse of `as_id()`)
    pub fn from_id(id: u64) -> PartNum {
        //TODO: error handling: 0 should not be accepted but should not cause a panic
//         assert!(id != 0, "check bounds on classification / partition number");
        PartNum { num: id >> 24 }
    }
}
impl From<u64> for PartNum {
    fn from(n: u64) -> PartNum {
        assert!(n != 0 && n < (1<<40), "check bounds on classification / partition number");
        PartNum {
            num: n
        }
    }
}
