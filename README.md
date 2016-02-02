Pippin
====

Pippin is a database inspired by distributed version control systems (notably
git). Unlike git it is designed to store thousands to millions (or more) small
objects in only a few dozen files. Unlike regular databases, it is designed
with distributed synchronisation in mind and convenient access to objects of a
single user-defined type. Pippin does not (currently) have a true index for
searching its database, but does have partitioning to reduce searches to a
smaller subset.

For more, see the documentation in [src/lib.rs]() or take a look at the [examples]().


Status
----------

Pippin is 'alpha' status. Some of the core features work fine:

*   persistance of data within a single 'partition' via snapshots and change logs
*   checksumming

Some are only partially there:

*   real partitioning support
*   merging of conflicting changes


Doc
----

The [doc]() directory contains some file-format documentation and various notes
planning Pippin's development.

The [doc/tickets]() directory contains a low-tech, distributed, off-line
capable issue-tracker.


Building, running, testing
-------------------------

Pippin uses [Cargo](http://crates.io/). A few example commands:

    cargo test
    cargo build --release
    cargo run --example pippincmd -- -h
    cargo help run
    cargo doc && open target/doc/pippin/index.html

Generated binaries can be found in the `target` directory.


## Licence

Pippin is licenced under the Mozilla Public License, version 2.0.
A copy of this licence can be found in the LICENSE-MPL2.txt file
or obtained at http://mozilla.org/MPL/2.0/ .

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the MPL-2.0 license, shall be
licensed as above, without any additional terms or conditions. 
