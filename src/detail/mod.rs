//! Pippin implementation details.

//! Many code forms shamelessly lifted from Alex Crichton's flate2 library.

mod sum;
mod header;
mod snapshot;

pub use ::detail::header::{read_head, write_head};

// Information stored in a file header
pub struct FileHeader {
    /// Repo name
    pub name: String,
    pub remarks: Vec<String>,
    pub user_fields: Vec<Vec<u8>>
}
