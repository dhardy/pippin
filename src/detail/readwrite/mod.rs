//! Pippin support for reading from and writing to files.

//! Many code forms shamelessly lifted from Alex Crichton's flate2 library.

mod sum;
mod header;
mod snapshot;
mod commitlog;

use std::{io, mem};
use error::{Error, Result};

pub use self::header::{read_head, write_head, validate_repo_name};
pub use self::snapshot::{read_snapshot, write_snapshot};
pub use self::commitlog::{CommitReceiver, read_log};

// Information stored in a file header
pub struct FileHeader {
    /// Repo name
    pub name: String,
    pub remarks: Vec<String>,
    pub user_fields: Vec<Vec<u8>>
}


// Utilities for reading from streams:
//TODO: replace this with Read::read_exact() when it's in stable.
fn fill<R: io::Read>(r: &mut R, mut buf: &mut [u8], pos: usize) -> Result<()> {
    let mut p = pos;
    while buf.len() > 0 {
        match try!(r.read(buf)) {
            0 => return Err(Error::read("corrupt (file terminates unexpectedly)", p, (0, 0))),
            n => { buf = &mut mem::replace(&mut buf, &mut [])[n..]; p += n },
        }
    }
    Ok(())
}
