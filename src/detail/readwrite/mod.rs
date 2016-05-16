/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Pippin support for reading from and writing to files.
//! 
//! Many code patterns shamelessly lifted from Alex Crichton's flate2 library.
//! 
//! Note that code in this module is not guaranteed to remain public. Probably
//! only some of the header items are useful, and of those only `UserData` is
//! needed (excepting some introspection like pippincmd does to read headers).

mod sum;
mod header;
mod snapshot;
mod commitlog;

pub use self::header::{UserData, FileHeader, FileType, read_head, write_head, validate_repo_name};
pub use self::snapshot::{read_snapshot, write_snapshot};
pub use self::commitlog::{CommitReceiver, read_log, start_log, write_commit};
