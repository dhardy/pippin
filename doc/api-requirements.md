API requirements
===========

Partititoning
----------------

It must be possible to split the repository into multiple partitions
automatically where the need arises. Partitions allow datasets to grow large
without requiring massive memory usage, massive files on disk and slow read
and write operations.

Data must be partitioned using some user-customisable properties so that where
applications require only some subset of elements loaded, they can avoid
loading a large proportion of the unrequired elements.

Paths are one possible approach to explicit partitioning aids, the idea being
that all data items are stored under some path explicitly and partitions are
created implicitly to span one or more paths. There is no real requirement that
paths be hierarchical, but there is a definite advantage in having some
explicit categorisation.


Load/initialise
-------------------

Operations required:

*   initialise a new repository
*   load an entire repository
*   load part of the repository according to properties used for partitioning
    (but selecting partitions automatically)
*   scan available partitions and list loaded partitions
*   load/unload specific partitions


Read support
------------------

Get item: given a full path/identifier, return the specified item in full.

Enumerate: given a path (some type of partition specifier), list identifiers
for all relevant items.

List: as enumerate, but including whichever details are asked for (e.g. name,
subject, full contents). From the point of view of the storage format, listing
any details besides identifier *may* require more work than simply enumerating
identifiers. Restricting the contents listed (e.g. to just a "subject" field)
will not be different, except that some kind of filter will be used to map full
details to those requested.

Search: given some possible restrictions on partitions, execute some selective
function capable of extracting desired contents and performing tests against
them, saving any details it wishes to some external container. Essentially this
is a convenience wrapper around the list functionality, except that allows easy
parallelisations (so long as whatever manipulations of external containers are
used don't result in too much locking/waiting).

List paths/partitions: return information on the paths or partition specifiers
used. Maybe list all at once or maybe only relative ones and not recursively.

List partitions: this is an implementation detail which the user shouldn't
really need to know.


Write support
-----------------

Add/replace item: given a full identifier and contents, insert a new item (or
possibly replace an existing item).

Modify item: rewrite some given field of an item.

Flush: should changes be written immediately or later? Or immediately to a
"temporary" file and later more permanently?


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
