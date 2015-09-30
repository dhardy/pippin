//! Support for reading and writing Rust snapshots

use std::{io};
use std::io::Write;
use chrono::UTC;
use ::Repo;
use ::detail::sum;
use ::error::{Result};

/// Write a snapshot of a set of elements to a stream
fn write_snapshot(elts: &Repo, writer: &mut Write) -> Result<()>{
    // A writer which calculates the checksum of what was written:
    let mut w = sum::HashWriter::new256(writer);
    
    try!(write!(&mut w, "SNAPSHOT{}", UTC::today().format("%Y%m%d")));
    
    // TODO: state checksum
    // TODO: per-element data
    // TODO: number of elements
    // TODO: time stamp
    // TODO: commit number?
    // TODO: checksum of data written
    
    Ok(())
}
