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
//! 
//! ### Relationship between structs and traits
//! 
//! Traits for the user to implement (only the first two if only using a single
//! partition; useful implementations of the "IO" traits are provided):
//! 
//! *   ElementT
//! *   PartitionIO
//! *   ClassifierT (depends on ElementT)
//! *   RepoIO (must be able to yield PartitionIO objects)
//! *   RepoT (requires ClassifierT and RepoIO implementations)
//! 
//! Library structures:
//! 
//! *   PartitionState (depends on ElementT)
//! *   Partition (uses a PartitionIO and yields PartitionState objects)
//! *   Repo (uses a RepoT and holds Partition objects, can yield RepoState objects)
//! *   RepoState (uses a ClassifierT object)

// Used for error display; not essential
#![feature(step_by)]

// Nicer syntax. Until there are no other issues preventing Pippin from
// building with stable code (namely, hashindexed), I see no reason not to use
// it. I hope Rust adopts this or finds a nice alternative.
#![feature(box_syntax)]

// This should probably be enabled by default for libraries.
#![warn(missing_docs)]

extern crate crypto;
extern crate chrono;
extern crate byteorder;
extern crate hashindexed;
extern crate regex;
extern crate vec_map;
extern crate rand;
extern crate walkdir;

pub use detail::{Repo, RepoIO, RepoState, RepoDivideError, RepoT};
pub use detail::{ClassifierT, ClassifyFallback, DummyClassifier};
pub use detail::{ElementT};
pub use detail::{PartitionState};
pub use detail::{Partition, PartitionIO, PartitionDummyIO};
pub use detail::{DiscoverPartitionFiles, DiscoverRepoFiles};
pub use detail::merge;
pub use error::{Result};

// Most Pippin code is put in this private module to allow inter-module
// dependencies without making the details public. In the future there may
// be other ways to do this (i.e. better privacy control).
mod detail;
pub mod error;
pub mod util;

/// Version. The low 16 bits are patch number, next 16 are the minor version
/// number, the next are the major version number. The top 16 are zero.
/// 
/// Until the library enters 'beta' phase this shall remain zero and nothing
/// shall be considered fixed.
pub const LIB_VERSION: u64 = 0x0000_0000_0000;
