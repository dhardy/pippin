<!-- This Source Code Form is subject to the terms of the Mozilla Public
   - License, v. 2.0. If a copy of the MPL was not distributed with this
   - file, You can obtain one at http://mozilla.org/MPL/2.0/. -->

API requirements
===========

Simultaneous access
---------------------------

Pippin is roughly a database. Like databases, it should support simultaneous
access from multiple parties or parts of a program.

It already does in a way: it is designed such that multiple instances accessing
the same files should not clash (though they may not synchronise until the next
load). But it should support in-memory parallel usage.

One option would be to put each access through a queing system, but this makes
each request slower.

Another would be to allow read access to multiple parties and copy-on-write
with some synchronisation system. This makes each element insertion/deletion
expesive and requires frequent synchronisation. It might be possible to reuse
old copies of the map (after synchronisation) once they no longer have read
locks; otherwise each batch of non-concurrent accesses requires a new copy.

Another would be to allow each user to make a copy of the current state and
only allow modifications through a copy. Committing modifications may require
a merge. The user should be able to check for external modifications while
holding a copy. This may be useful for a form of transactions.


Repositories
-----------

There should be an interface representing a single partition's data in memory;
call this `Repository`.

A `Repository` should be able to represent multiple states of its data; this is
needed for commit replay and commit creation, as well as for history browsing.
In particular, a `Repository` must represent at least two states (which may be
equal): the current state, and the last saved state.

Interfaces for creating a `Repository`:

1.  Create empty, with a name
2.  Load all snapshots and commit logs provided by some interface
3.  Ditto, but with restrictions (e.g. only latest state)

Interfaces for modifying a `Repository`:

1.  Load more snapshots/logs provided initially, optionally with restrictions
2.  Modify the current state, by:
    
    *   inserting an element
    *   replacing an element
    *   deleting an element
3.  Create a new commit from the current state
4.  Writting all changes to a commit log (automatically choosing whether or not
    to additionally create a new snapshot)
5.  Write a new snapshot

Interfaces for reading data from a `Repository`:

1.  List element identifiers
2.  Iterate over elements, perhaps with filters
3.  Retrieve a specified element

Note that this is incomplete: some mechanism is required in order to (a)
provide snapshot and commit log data streams for loading, (b) provide a data
stream for writing a new snapshot, (c) provide a data stream for writing a
commit log as well as removing obsolete commit logs.


### Repository file discovery

There should be some interface for discovering repository snapshots and log
files given a path to a snapshot file, either limited to the specified snapshot
file plus its commit logs, or resolving all snapshots and commit logs for the
repository.

Creation:

1.  Snapshot only, via path
2.  1 + find corresponding commit logs
3.  Extrapolate to all files for the repository

Interface: this should implement some trait used by `Repository`, allowing
retrieval of the latest snapshot, all snapshots in historical order, commit
logs for each snapshot (maybe via a sub-interface), creation of new snapshot
files (more accurately writable streams), and creation of new log files.


Maintenance operations
-------------------------------

Push/pull/merge: push local modifications to a remote copy, pull remote
modifications, merge changes (only automatic ones or start manual merge), etc.

Fix: if checksum errors are found, try to recover (e.g. check whether remote
copies are also corrupted, try to localise the corruption, possibly ask the
user, replay a series of patches and compare to a snapshot).


Encryption
--------------

I don't know what might be needed here, or maybe combined elsewhere...
