//! Pippin (sync-sets) library

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

pub use detail::Repo;
pub use detail::{Element};
pub use detail::{PartitionState};
pub use detail::{Partition, PartitionIO, PartitionDummyIO, TipError};
pub use detail::DiscoverPartitionFiles;
pub use error::{Error, Result};

pub mod error;
pub mod util;
mod detail;

/// Version. The low 16 bits are patch number, next 16 are the minor version
/// number, the next are the major version number. The top 16 are zero.
pub const LIB_VERSION: u64 = 0x0000_0000_0000;
