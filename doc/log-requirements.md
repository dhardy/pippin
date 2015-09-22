Log requirements
==========

Premise: some log format is needed to store change history and possibly element contents
(something between options 3 and 4 listed in the *Requirements* document).


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


Element identifiers
--------------

How should elements be identified?

**By name?** Using a user-defined name would for example make it possible to
identify the partition containing data associated with some element while only knowing
the start of the name, but is useless if e.g. the start isn't known or an element is searched
for by some other criteria.

**By checksum of name?** This helps avoid biased partitioning, but makes searches
by name harder.

**By some unguessable checksum or random key?** Since searches by full contents
should be possible in any case, there may not be much advantage to making identifiers
predictable.

**By time of creation?** This would aid in making partitioning of elements into
subsets useful, in that one could for example quickly list all mails received recently
without worrying about archives from last month/year; however finding old messages
still contained in the inbox would be slower.

### Reasoning of possibilities

Elements need to have an identifier for fast and precise identification (a) for
use in memory and (b) for use in commits and snapshots in the save files. In
files, it would in theory be possible to identify elements by their checksums,
but this would require using an extra lookup table while reconstructing the
state from a snapshot and commits. In memory, one could:

1.  Put all elements in a vector and use the index. In theory this should work
    but it might only take a small bug to cause the wrong element to get
    selected.
2.  Use some user-defined classifier based on the element's properties. Doable,
    but in order to be practical the domain needs to be fixed (e.g. `u64`) and
    the value guaranteed unique. Guaranteeing uniqueness may require perturbing
    the element, which might require attaching a random number field or similar
    and, since elements should not change other than when explicitly modified,
    this requires that the perturbation be done when adding or modifying an
    element and that the classification function does not change.
3.  Attach some "id" property for this purpose.

Option 1 is a little fragile. Option 2 is complex, a little fragile, and might
slow loading of the file to memory. Option 3 is therefore selected, also
because it avoids the need for an extra lookup table during loading.

Identifiers could be assigned randomly, sequentually or from some property of
the element (e.g. from a checksum). I see no particular advantage any of these
methods has over others save that sequentual allocation might make vec-maps
usable but requires more work when combining partitions.

Note: the identifier is only required to be unique to the API "partition" and
the file; further, since not all partitions may be loaded, it may not be
reasonably possible to ensure global uniqueness. This implies that if
partitions are ever combined, it may be required to alter identifiers, which
means either there must be an API for this or identifiers must be assignable
by the library.


Partitioning
------------

For a small enough set, a single log file will be sufficient. At some point, partitioning
the log becomes essential. For the reasons outlined in the *Requirements* document,
both partitioning history and partitioning elements should be allowed.

Additionally, it should be possible to find all elements in some user-defined subset (e.g.
a folder or tag) quickly. Partitioning could take advantage of exclusive user-defined subsets
(e.g. folders, but not necessarily tags), *however* the user may move an element to
another subset, which makes history tracking difficult. It may be better not to partition
on this type of thing, but either rely on full-content searches or on separate lists to
find elements by user-defined subset. If separate lists are used, it may be simpler to
make these simply caches and not try to synchronise them or protect their contents
from corruption.

On the other hand, it may make some sense to partition according to user-defined
subsets; for example a common use would be for an "inbox" folder, and it would be
desirable to allow its contents to be quickly found without having to scan all elements
(which may include tens of thousands of archived items).

Further: it is required that data can be partitioned according to some
user-defined criteria (e.g. folders) and that the current state of a partition
can be read and edited quickly even if the entire data-set or history is huge.

Rewriting: it would probably make sense if old entries are never removed from
the history log but can be "deleted" from some partition by a new log entry.

### Partitions, files and identification

What is a *partition?* Is it (1) the elements and history stored within a
single file, or (2) the sub-set of elements which are stored in a single file
but including the full history in previous and possibly more recent files?

From the API perspective, it may sometimes be required to know which segments of
history are loaded, but for the most part the user will not be interested in
history thus the API will be simpler if a "partition" in the API refers to a
sub-set of element and ignores history.

There needs to be some file naming convention and way of referring to
partitions within the API; each element must be uniquely identified by the
(partition identifier, element identifier) pair.
Requirements:

*   There must be a map from elements to partitions which is deterministic and
    discoverable without loading the partition data or determining which
    elements exist.
*   It is required that a file clearly identifies which elements it may contain in
    either the file name or the header.
*   It is required that there be a global method of designing new partitions
    when required.
*   It is strongly preferred that the library have a set of user-defined
    classifiers on elements that it may use in a user-defined order to design
    new partitions, and that these classifiers provide sufficient granularity
    that partition sizes are never forced to be larger than desired.


Encryption
-------------

Possibly log-files should have integrated support for encryption. But what should be
encrypted? Obviously element contents, but what about time-stamps of changes
and information on the partitioning (which elements and time-frame)?

Is it advantageous to encrypt the contents of some parts of the file as opposed to
encrypting the whole file on disk? Possibly, since we want to be able to append data
to the file without rewriting the whole thing.

How should the encryption work? Store a symmetric key at the start of the file,
encrypted by some external key or some password?

How many people will want to encrypt the mail on disk anyway, and not simply
by whole-disk encryption?


Compression
--------------

Since a log should consist of a large number of short pieces of data, it seems to
make most sense if compression is done only at the level of whole files, the same
as with encryption.

However, since the intention is that log files can be extended bit-by-bit without
requiring rewriting the whole file, it may make sense that the file is divided into
"blocks" (e.g. the header, then a number of fixed-size blocks each containing log
entries until full), with each block compressed independently.


Structure
-----------

A log file needs to represent a set of elements, some of which may change over
time. It is the intention that the log may be written only by appending data and
modifying a fixed-length header.

Option 1: no index. Finding elements requires parsing the entire file; if read in
forward order element history is read oldest-to-newest.

Option 2: fixed size header accommodating a set number of elements; for each some
identifier (maybe just a number) and some address within the file would be stored.
The address would point to the latest log entry for the element; each log entry
would contain the address of the previous log entry. When the header runs out of
room for new elements a new log file must be started.


Compaction
----------------

It may well be desirable to "rewrite history" in a way that combines change
records in order to reduce the size of the historical record, possibly even
across partitions, in order to reduce disk space.

New log entries would be per-event; compaction could for example reduce this to
daily, weekly, monthly or annual entries. Entries added then deleted within the
time-frame of a single entry would be lost entirely. When entries become
sufficiently large, an entry may simply be a complete snapshot of all entries
in the partition instead of some kind of "diff" patch.

When compaction is done on one node, other nodes synchronising with it would
have four options: keep extra history locally (no change), re-do compaction
locally (expensive on the CPU), inherit compaction blindly (expensive download,
increased chance of corruption), or push local history to the remote node
(undo compaction, assuming history on local node is correct).

It would be desirable that two nodes performing compaction independently but
then synchronising would end up with the same result, even if compaction were
done in multiple stages on one node. There should be some fixed algorithm for
choosing which time-points to keep.


Checksums
---------------

Checksums should be added such that (a) data corruption can be detected and (b)
replay of log-entries can be verified.

State checksums: a deterministic method of producing a single checksum on the
full current state of a partition (data of all entries). Question: combine entry
data in a commutative method or not? For: easier parallelisation of calculation,
should be possible to calculate the new checksum from the old and a knowledge
of changed entries; against: nothing?


Evaluation of options
==============

Following on from the above, we evaluate possible options.


Requirements summary
------------------------------

One file stores a "partition" of full data set, potentially restricting both
vertically (time frame covered) and horizontally.

Files must store changes (potentially all changes within a partition).

Compression required _somewhere_. Checksums also required.

It must be possible to make changes by only changing the end of the file
and possibly the header.


Options for storing history in partition files
-----------------------------------------------------------

### Snapshot + changesets

Obviously it's undesirable to have to read all history to reconstruct the
current state, yet maintaining full history is desired. The simplest solution
is to "cache" the full state at various points in time into a _snapshot_ while
keeping a log of all changes.

The simplest way of implementing this would be that each file contains a
snapshot and changesets following the snapshot. This also meets my data-loss
requirements.

Open question: is it worth supporting snapshots at any point *after* the start
of a partition (i.e. file), i.e. allowing new snapshots without starting a new
file/partition?

### Running state

Another approach: maintain the latest state in memory, and write this out to
disk every so often as a "check point". On next load recover from the latest
check point plus log entries.

This requires more writes to disk but may achieve faster loads. There may be a
higher chance of corruption, but by using checksums and keeping multiple
checkpoints it _should_ be okay.

This could be an option _on top of_ snapshots stored at the start of new
partitions. Further, since there is an increased chance of corruption, I will
not pursue this option unless performance tests later show that it could be
useful.

### Add an index?

This is orthoganol to the above options. Should an index be kept?

For this: it allows reading a sub-set of messages without reading the entire
file. It could make reconstructing the latest state from a large log faster
(at least, on disks happy to do lots of seeking, and this is only really
advantageous for very large logs, which should probably be partitioned anyway).

Against: it means space has to be preallocated in the header, and either the
header expanded or a new partition started when that space is used up. I also
don't see much point since the system is designed only to hold small pieces
of data (and is intended itself to be an index of sorts).

### Support moving items?

If a message is moved into a different folder or under a different tag, should
this system support moving items in any other way than recreating them?

To collect all information on items matching a certain tag or path including
those not originally created with that tag/path, without collecting information
on all items (up to the point of deletion), and without having to re-read (do
a second pass), it will be required to list items in full if a tag is added or
path changed.

Suggestion: if the primary key (used to organise items between partitions)
changes, then list a "moved to ..." log entry in the old partition and a "moved
from ..." entry including the full state in the new partition (which may be the
same one). Possibly also list the full state when some application-specific
property of the data changes is true.


Appending new commits
----------------------

There is an issue with simply appending data to a file: if the operation fails,
it might corrupt the file. It is therefore worth looking at using multiple
files.

Each partition will thus have multiple files used to reconstruct the current
state as well as potential historical files. This might make it worth using a
sub-directory for each partition.

**Separate log files:** since log files will need to be updated more frequently
than snapshots and snapshots may be large, it makes sense to store them in
separate files.

**Snapshots:** a new snapshot may be written whenever log files are deemed
large enough to start fresh. The snapshot files need not be duplicated; in the
case that a snapshot file appears corrupt, the program may reconstruct the
state from a previous snapshot plus log files.

**Log files:** whichever log file(s) are required to reconstruct the current
state should not be altered. New log entries should either be written in new
log files or by altering a redundant log file; either way the new file should
contain all entries from the other log files. Reconstructing the new state
should read all log files involved then mark redundant files for deletion or
extension.

A **merge** could be done from multiple log files in the same location or by
pulling commits from some other source; either way the merge should proceed by
(1) reconstructing the latest state of each branch, (2) accepting changes which
do not conflict and somehow resolving those which do, (3) creating a special
"merge commit" which details which changes were rejected and any new changes,
and (4) writing all commits involved to the log, marking them with a branch
identifier so that reconstruction is possible.

Dealing with **corruption:** if a snapshot is found to be corrupt, the program
should try to reproduce history up to that point, and if successful rewrite the
snapshot. If a commit is found to be corrupt, the program should try to find
another copy of that commit, either locally or in a clone of the repository.
When these strategies fail, things get complicated (user intervention, best
guesses or dropping data).


Contents of items in the data set
-----------------

For the email application, we probably need key-value pairs so that changes
affecting different keys can be merged easily. At any rate, there needs to be
some way of merging conflicting changes (doing a three-way merge),

Option 1: always store the full state and allow an application-specific
algorithm to be used to merge conflicting new states.

Option 2: allow application-specific algorithms to generate patches and apply
them, along with some back-up handler if patching fails when merging.


Checksumming
--------------------

Of elements/items: may be useful to allow faster calculation of state checksums
but I see little other use.

Of file header, snapshot, and log entries: this allows detection of file corruption.

State checksums option 1: XOR of a checksum of each item. Simple, easy to
parallelise calculation, possible to calculate from a change log without full
data. Against: relies on strength of underlying checksum to prevent intentional
collisions (which *might* allow history alterations during merges).

State checksums option 2: checksum of concatenation of all data items. Possibly
more secure but requires full data set to calculate.
Reduces meta-data stored for each item in the set.

State checksums option 3: store a less secure checksum for each data item (e.g.
SHA-1 160-bit or even MD5 128-bit) then calculate a secure checksum of the
concatenation of each item's checksum. Possibly more secure than option 1, or
possibly less: one data item could be changed to something whose checksum
collides with that of the original without detection.

Note: options 2 and 3 can support parallelisation by calculating in a pyramid
fashion (e.g. contatenate 64 items and calculate checksum, then concatenate
64 of those checksums...).
