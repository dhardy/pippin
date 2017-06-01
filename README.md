Pippin
====

Pippin is a database inspired by distributed version control systems (notably
git). Unlike git it is designed to store thousands to millions (or more) small
objects in only a few dozen files. Unlike most databases, it is designed to
allow independent distributed usage with eventual (potentially user-assisted)
synchronisation.

Pippin is an *object database* in that complex objects (any supporting
serialisation) are stored directly, not a *relational database* using
predefined numeric and string types. Pippin is *serverless*, designed to run
in a single process, or even from multiple processes simultaneously using
the same files on disk (with eventual synchronisation).

ACID-compliant transactions are possible (with a single DB host and within a
single partition), depending on how merges are handled; merge functionality is
highly configurable.

The full history of transactions/commits is recorded via a combination of
*snapshot files* and *commit logs*. Snapshot files never change and logs are
kept small, making for simple backups.
Old snapshots and logs can simply be deleted if the history is no longer required.

Pippin is designed to be scalable via partitioning, achieved using user-defined
classifiers. This functionality, along with filtering (i.e. simple search), is
planned, but not yet available.

For more, see the documentation in [src/lib.rs](src/lib.rs) or take a look at
the [examples](examples/).


Change-log
----------

### Pippin 0.1.0

Pippin is 'alpha' status.

Partition-oriented usage (i.e. a single 'partition') should have all the basic
features there and is ready for testing, but the API may change. Perhaps the
biggest caveat is that every commit is written to a new file due to not yet
working out how to safely extend files.

Repository-oriented usage is not yet ready.

What should work:

*   persistance of data within a single 'partition' via snapshots
*   storing changes via commit logs
*   reconstruction of state from snapshot + logs
*   auto-detecting latest state(s)
*   merging of multiple latest states (may require user-interaction)
*   checksumming & detecting corrupt stuff
*   recovery of some data when files are missing (though this needs more work)
*   file formats are mosly final except that headers will get extra data and object diffs

What is planned:

*   tracking mutliple partitions in a distributed manner via file headers
*   user-specified classifiers
*   (possibly) indexes of some kind
*   reclassification of objects as necessary
*   partially-automated division of "large" partitions via classifiers
*   object diffs (current commits include a full copy of all changed entries)
*   log file extension (currently a new file is used per commit to avoid data loss)


Doc
----

The [doc](doc/) directory contains some file-format documentation and various notes
planning Pippin's development.

Tickets were originally stored in files. Several "tags" are still in use; where
applicable these are mentioned in tickets and can be used to find relevant bits
of code. All of these can be found with grep:

    egrep -IR "#00[0-9]{2}" doc/ src/


Examples & tests
-----------------------

Some self-contained examples can be found in the `examples` and `tests`
directories:

    examples/hello.rs       — minimal example
    examples/pippincmd.rs    — tool to read/write DB entries as plain text
    app_tests/examples/sequences.rs — test program generating random DB entries
    
    tests/partition-ops.rs  — external test suite for partition operations
    app_tests/tests/seq_create_small.rs — create a small random repo as a test

More examples and tests can be found in the `applications` directory. These
make use of an extra library including some common code.


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
