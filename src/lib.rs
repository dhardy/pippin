/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

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
//! TODO: scalability. The current code requires reading all data on start-up;
//! the original approach (partitioning) was abandoned for a host of different
//! reasons; an alternative approach (reading data on-demand) is planned.
//! 
//! Historical data may be deleted easily, since full snapshots are written
//! periodically. The limitation here is that distributed synchronisation
//! requires common history; currently it is up to the user to ensure that
//! sufficient common history is maintained on machines doing merges.
//! 
//! The library has good support for checking for corruption of data, though
//! currently limited facilities for dealing with corrupt data.
//! 
//! Potentially, it could be extended to support the following, however so far
//! there has been no need for these features:
//! 
//! *   Multiple branches within a single repository (like 'git branch')
//! 
//! Terminology:
//! 
//! *   **repo** — **repository** — the set of objects and history stored by a
//!     single instance of the library
//! *   **elt** — **element** — an object stored in a repository
//! *   **state** — a consistent view (version) of data within a repository
//! *   **commit** — a change-set used to update one state to the next
//! *   **snapshot** — a file recording all data in a single state, created
//!     periodically primarily for performance reasons, redundant with previous
//!     snapshot + commit logs
//! *   **commit log** — a set of commits applying on top of some snapshot;
//!     a snapshot and all associated commit logs are combined to reproduce
//!     the latest state
//!
//! Usage should be via the `Repository` type. See `examples/hello.rs` for a
//! simple example.
//! 
//! ### Main traits and structs
//! 
//! TODO: rename PartXXX
//! 
//! These traits allow user control; default implementations are normally
//! available, although you will probably want a custom implemetnation of
//! `Element`.
//! 
//! *   `Element` — data type stored
//! *   `PartIO` — provides access to data via filesystem or other source
//! *   `PartControl` — depends on `Element`, provides access to `PartIO`,
//!     controls various options and optional features
//! 
//! Primary structs:
//! 
//! *   `PartState` — a consistent view of data
//! *   `Partition` — represents states, controls loading and saving of data

// This should probably be enabled by default for libraries.
#![warn(missing_docs)]

// Stupid warning.
#![allow(unused_parens)]

extern crate crypto;
extern crate chrono;
extern crate byteorder;
extern crate hashindexed;
extern crate regex;
extern crate vec_map;
extern crate rand;
extern crate walkdir;
#[macro_use]
extern crate log;

pub mod commit;
pub mod elt;
pub mod error;
pub mod io;
pub mod merge;
pub mod part;
pub mod pip;
pub mod rw;
pub mod state;
pub mod sum;
pub mod util;


/// Version. The low 16 bits are patch number, next 16 are the minor version
/// number, the next are the major version number. The top 16 are zero.
/// 
/// Until the library enters 'beta' phase this shall remain zero and nothing
/// shall be considered fixed.
pub const LIB_VERSION: u64 = 0x0000_0000_0000;
