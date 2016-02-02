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
slow and requires frequent synchronisation.

Another would be to allow each user to make a copy of the current state and
only allow modifications through a copy. Committing modifications may require
a merge. The user should be able to check for external modifications while
holding a copy.


Partitions
-----------

There should be an interface representing a single partition's data in memory;
call this `Partition`.

A `Partition` should be able to represent multiple states of its data; this is
needed for commit replay and commit creation, as well as for history browsing.
In particular, a `Partition` must represent at least two states (which may be
equal): the current state, and the last saved state.

Interfaces for creating a `Partition`:

1.  Create empty
2.  Load all snapshots and commit logs provided by some interface
3.  Ditto, but with restrictions (e.g. only latest state)

Interfaces for modifying a `Partition`:

1.  Load more snapshots/logs provided initially, optionally with restrictions
2.  Modify the current state, by:
    
    *   inserting an element
    *   replacing an element
    *   deleting an element
3.  Create a new commit from the current state
4.  Writting all changes to a commit log (automatically choosing whether or not
    to additionally create a new snapshot)
5.  Write a new snapshot

Interfaces for reading data from a `Partition`:

1.  List element identifiers
2.  Iterate over elements, perhaps with filters
3.  Retrieve a specified element

Note that this is incomplete: some mechanism is required in order to (a)
provide snapshot and commit log data streams for loading, (b) provide a data
stream for writing a new snapshot, (c) provide a data stream for writing a
commit log as well as removing obsolete commit logs.


### Partition file discovery

There should be some interface for discovering partition snapshots and log
files given a path to a snapshot file, either limited to the specified snapshot
file plus its commit logs, or resolving all snapshots and commit logs for the
same partition.

Creation:

1.  Snapshot only, via path
2.  1 + find corresponding commit logs
3.  Extrapolate to all files for the partition

Interface: this should implement some trait used by `Partition`, allowing
retrieval of the latest snapshot, all snapshots in historical order, commit
logs for each snapshot (maybe via a sub-interface), creation of new snapshot
files (more accurately writable streams), and creation of new log files.


Repositories
--------------

An interface for representing a multiple-partition repository in memory is
required; call this `Repository`.

A repository could be created:

1.  Empty, with a name and some classifier functions (possibly empty)
2.  Loading partitioning data given some data source

It could then provide:

1.  A list of partitions available and partitions loaded

and could be modified by:

1.  Loading all partitions at the latest state
2.  Loading or unloading a specific partition
3.  Per element operations, which optionally load on demand:
    
    *   iterate over elements according to some filters
    *   insertion of an element
    *   deletion of an element
    *   replacement of an element
    *   retrieval of an element by id
4.  Adding a classifier function
5.  Reprioritising classifier functions
6.  Repartitioning some or all data
7.  Committing all changes

### Repository discovery

There should be some interface for discovering partition files given a
repository's location (directory or any one snapshot).

It would implement a trait provided alongside `Repository`, returning a list of
partitions found, each as a partition file discovery object. It would also be
used to store new partitions.

Note: this leaves interpretation of partitioning to `Repository`, not the
discovery tool. This is better for the case where the discovery tool is not
used.


Maintenance operations
-------------------------------

Compact data stores: rewrite some stored partitions, possibly combining some
patches / discarding some states in line with revised history requirements.

Push/pull/merge: push local modifications to a remote copy, pull remote
modifications, merge changes (only automatic ones or start manual merge), etc.

Fix: if checksum errors are found, try to recover (e.g. check whether remote
copies are also corrupted, try to localise the corruption, possibly ask the
user, replay a series of patches and compare to a snapshot).


Encryption
--------------

I don't know what might be needed here, or maybe combined elsewhere...
