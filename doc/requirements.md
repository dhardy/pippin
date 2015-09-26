Requirements of synchronised sets
==================

Synchronisation sets will be used for managing messages. Elements will contain:

*   tags used to organise messages (like gmail tags)
*   possibly the subject line or other short description
*   maybe keywords used for searching
*   identifiers of data blobs storing message content

1.  Needs to represent a set
2.  Within each element, partial changes need to be propagated
3.  It should be possible to partition the set, should it be impossible to load
    the whole thing into memory, with some user control over how items are
    partitioned
4.  Sufficient history to allow synchronisation must be saved. Since it may not be known when
    another copy may be branched from, some features may be required to be saved indefinitely.
5.  The system should be optimised for the case where a lot of elements exist with a lot of
    history, access to the contents of all elements in their current state is frequently needed,
    and the size of the contents of elements is small (not significantly larger than the
    history log).


Basic requirements
---------------

The log and element contents need to be representable as one or more files on disk.
To avoid needing many seeks and reads, the number of files should not be excessive
(thousands of small files); at the same time it must be possible to partition contents
into multiple files as outlined below.

It must be possible to clone the log and contents both locally and remotely. When doing
so, it must be possible to choose between cloning all available history and cloning only
from some more recent cut-off point.

It must be possible to synchronise a set and its clone, or two clones of the same original
set, provided that both maintain history back to some common ancestor. Changes which
were identical in both sets and changes which happened only in one set and do not clash
with another change to the same element must be merged automatically; it must be
possible to merge changes which clash manually or via some sufficiently smart logic.

It must be possible to compress data both on the disk and when transmitted.

It must be possible to tell by reading only a short header whether a log file
would contain data associated with a certain element key were that element to
exist during the time-frame covered by the log, and the start of the time-frame
covered by the log.

It should be possible to make changes to a log file only by appending data, possibly
with edits near its end. It may or may not be a good idea to edit the start of a log
file, e.g. to record the date of the last log entry or to include an index.

If one log file is lost, data-loss should be localised; in particular (1) elements not
covered by this file should not be affected; (2) if this file is not the latest for this
subset of elements then the state of the elements from the time at which the next
log-file starts should not be lost and synchronisation from this time point should
not be impaired; and (3) if a log-file covering the history of these elements prior
to this file still exists, then the state of the elements during this time frame should
be preserved.

Files should maintain internal checksums.

Where a large number of elements are changed at the same time within a subset,
it should be possible to represent the change via a single entry in the history log.

It may be useful for each entry in the log to mention the previous log entry which
mentions each of the affected elements to facilitate history building/searching.

Should the design assume the entire latest state will always be loaded into
memory? Fast access to all mails will require this, but it may make more sense
on some devices to load only some mails (such as those in the inbox) into
memory and do full searches from disk or while temporarily loading all mails.
It should in any case be possible to load the latest state of some partition
quickly.

It should be possible to handle data-sets of at least hundreds of thousands of
nodes, if not millions, and to quickly read the entire current state of some
partition of the data regardless of how big the full data set is. These
partitions should be customisable, e.g. folders automatically set up by
time-frame, by subject or by sender.
