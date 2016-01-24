//! Pippin library
//! 
//! Pippin is somewhere between a distributed version control system (e.g. git,
//! Mercurial) and a database. It is optimised for holding many small objects.
//! It tracks the history of these objects, enabling synchronisation of
//! changes from multiple sources (and potentially multiple repositories)
//! without requiring any central repository.
//! 
//! Objects may be of any user-defined type, such that (a) the same type is
//! used for all objects (dealing with polymorphism internally if required),
//! (b) objects are normally read-only with explicit copy-on-write, and (c)
//! objects can be serialised to and deserialised from a byte stream.
//! 
//! To scale well to large numbers of objects and changes, data is partitioned
//! both by time (history) and by classifying objects into sub-sets. This
//! classification is customisible and is currently the only means to speed up
//! search and filtered listing of objects.
//! 
//! Historical data may be compacted (reducing the number of states remembered)
//! and removed completely so long as common ancestor states can still be found
//! when merging. Where data is partitioned by classifier, not all partitions
//! are required to be available, so long as operations do not need to insert
//! or retrieve data on non-present partitions.
//! 
//! The library has good support for checking for corruption of data, though
//! currently limited facilities for dealing with corrupt data.
//! 
//! Potentially, it could be extended to support the following, however so far
//! there has been no need for these features:
//! 
//! *   Multiple branches within a single repository (like 'git branch')
//! *   Indexes of stored objects
//! 
//! Terminology:
//! 
//! *   **repo** — **repository** — the set of objects and history stored by a
//!     single instance of the library
//! *   **part** — **partition** — one sub-set of objects determined by a
//!     user-defined classifier, along with its history
//!
//! Usage should be via the `Repo` type or, for a simpler interface where
//! classification and partitioning is not required, via the `Partition` type.

// because at this stage of development there's a lot of it:
#![allow(dead_code)]

// Used for error display; not essential
#![feature(step_by)]

#![feature(box_syntax)]

#![warn(missing_docs)]

extern crate crypto;
extern crate chrono;
extern crate byteorder;
extern crate hashindexed;
extern crate regex;
extern crate vec_map;
extern crate rand;
extern crate walkdir;

pub use detail::Repo;
pub use detail::{ElementT};
pub use detail::{PartitionState};
pub use detail::{Partition, PartitionIO, PartitionDummyIO};
pub use detail::DiscoverPartitionFiles;
pub use error::{Result};

pub mod error;
pub mod util;
mod detail;

/// Version. The low 16 bits are patch number, next 16 are the minor version
/// number, the next are the major version number. The top 16 are zero.
/// 
/// Until the library enters 'beta' phase this shall remain zero and nothing
/// shall be considered fixed.
pub const LIB_VERSION: u64 = 0x0000_0000_0000;
