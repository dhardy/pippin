Technical design of sync-sets
====================

File contents
-------------

Header:

*   identify file format and version (possibly a searchable keyword)
*   identifier of previous history file (later)
*   information on which items of set are contained in this partition (later)
*   some identifier for the repository?
*   extension support
*   checksum of header (single or multiple?)

Snapshot:

*   listing of all items in the partition with full data
*   data (as in file) checksum and state checksum

History log (commits):

*   identifier (checksum of commit like git? some kind of version number?)
*   timestamp
*   for each item changed: checksum of current contents, full contents (later:
    patch support), identifier of last checksum where it changed, identifier
    for item
*   checksum of whole commit
*   state checksum of partition after this change
*   (possibly) state checksum for before the change


Identifiers
------------------

State sums: these are checksums which (1) identify a state reached by a snapshot
or commit, and (2) validate data in a recreated state. They can be used to find
snapshots and commits, but there may be multiple snapshots and commits for a
single state-sum.

Note that if a commit ever reverts to a previous state, this will create a loop
in the history graph, rendering the reverting commit redundant and (unless
there is a snapshot within the loop) useless. Code should deal with this.

Commit identifiers: use the commit checksum as an identifier like git etc.
This may not be required, since normally one would only request a state and not
care how it is reproduced.

Rationale: commit identification is important for merges. When however it comes
to deleting old history, commits will either be forgotten entirely or merged
into larger commits with new checksums. State checksums, however, will remain
the same (for those states which survive).

Printing state sums and commit checksums: use base 36 (0-9, a-z; i.e.
hexadecimal extended to the end of the alphabet). Always use lower-case letters
to avoid confusion between zero (`0`) and upper-case `O`. A 256-bit number
requires only 50 characters in base-36 (or 64 in base 16). Like git, accept
abbreviated sums so long as these are unique within known history.

Initial state: give it a special identifier, all zeros (i.e. the result of XOR
combining zero element checksums).

### Element identifiers

Element identifiers are a unique `u64` (unsigned 64-bit number). The first
32 bits (high part) is fixed for the partition or some subset of the partition
(TBD). The low 32 bits are simply a number unique within the subset using the
high 32-bit number. (Example: a partition might use 123 as the high part. A new
element can be assigned any 32-bit number unique within the partition, e.g. 5.
The element identifier would then be 123 × (2^32) + 5.)

### Snapshot and commit log file names

See *identifiers* section under *partitions*, below.


Element data
------------

For now this is just a binary byte array. TBD.


Partitions
-------------

A *partition* is defined from the API point of view: a subset of elements which
are loaded and unloaded simultaneously. The partition-file relationship is
1-many: any particular state of the partition's history is fully encapsulated
in a single file, but the full history may span multiple files. Any file will
contain the entire state of some partition at some point in time, and likely
multiple points in time via changesets and/or multiple snapshots.

*Partitioning* of the repository into partitions is neither user-defined nor
fixed. User-defined classifiers are used when designing new partitions in a
user-defined order. A partition will be defined according to some range of
allowable values from one or more classifiers. If insufficient classifiers are
present to give desirable granularity of partitions, the library can do no more
than warn of this.

### Classifiers

Classifiers are user-defined functions mapping elements to a number, which is
either a user defined enumeration or a user-defined range of integers (TBD:
32-bit? signed?). Each function must have a name (TBD: allowable identifiers).

The names of classifier functions and their domains (enumeration or integer
range) are recorded in each snapshot and may not change. The functions
themselves are provided by the user and *should* not change; if they do, the
library will provide options for dealing with this (search by iteration over
all elements of a partitioning, reclassification of all elements in a
partition), but searching/filtering by classifiers may not return the correct
elements until reclassification is complete.

Classifier functions may not be removed (as a work-around, they can be changed
to output only a single value and values reclassified). New classifier
functions may be added with lowest priority but will only be recorded in new
snapshots.

It is an error if snapshots do not agree on classifier function domains with
either other snapshots or the user-provided functions. It is an error if
snapshots record classifier functions not provided by the user. In either case,
the library may provide only limited functionality (e.g. read-only with limited
search capabilities or require reclassification of all partitions), or the
library may simply crash with a suitable error message.

Priority of the classifier functions is user defined. This is recorded in
snapshots but may be changed; however partitioning will not be changed until
either the user or the library decides to repartition.
In the mean time, existing snapshots may or may not be updated
with the new priorities while new snapshots will record the new priorities but
continue to represent the old partition.

### Identifiers

TBD: how to identify a partition in memory.

Given some `BASEPATH` (first part of the file name, potentially prefixed by a
path), snapshot files are named `BASEPATH-ssN.pip` where `N` is the number of a
snapshot. The first non-empty snapshot is usually numbered `1`; subsequent
snapshots should be numbered one more than the largest number in use (not a
hard requirement). The convention for new partitions is that an empty snapshot
with number `0` be created.

Commit log files correspond to a snapshot. A commit log file should be named
`BASEPATH-ssN-clM.piplog` where `M` is the commit log file number. The first
log file should have number `1` and subsequent files should be numbered one
greater than the largest number previously in use.

This `BASEPATH` is an arbitrary Unicode string except that (a) it may contain
path separators (`/` on all operating systems), and (b) the part after the last
separator must be a valid file name stem and the rest must either be empty or
a valid (relative or absolute) path. To aid users, names may be suggested by
the program using the library.

For now, partition data files are restricted to names matching one of the 
following regular expressions:

    ([0-9a-zA-Z\-_]+)-ss([1-9][0-9]*).pip
    ([0-9a-zA-Z\-_]+)-ss([1-9][0-9]*)-cl([1-9][0-9]*).piplog


Checksums
----------------------

Algorithm 1 (file corruption detection, commit identifiers): must be fixed
unless file headers are re-read *after* checking the algorithm. Desirable that
it is not easy to fake a commit with a clashing identifier to another. Is SHA-1
suitable?

Algorithm 2 (state checksums, verify correct reconstructions): does not need
to be fixed. Possibly strength is less important?

Algorithms: MD5 (and MD4) are sufficient for checksums. SHA-1 offered
security against collision attacks, but is now considered weak. SHA-2 and SHA-3
are more secure; SHA-2 is a little slower and SHA-3 possibly not yet
standardised. SHA-256 is much faster on 32-bit hardware but slower than SHA-512
on 64-bit hardware.

Following [this advice](http://stackoverflow.com/a/5003438/314345) I will use
SHA-256 for now.

Git and many other DVCSs use SHA-1 and store the full 160-bit output. Where
security is not important, I don't see any issue with using this or even MD5;
SHA-2 256 or SHA-3 256 would however seem a sensible default as the algorithms
are significantly more secure without upping storage requirements massively.

At the moment I see maybe a little reason to consider security (with low
security it might be possible to rewrite history when a merge is done from a
compromised host; I don't know), and little reason to skimp on security
(for corruption detection SHA-3 is excessive; I don't know whether CPU
performance or storage of large checksums will be significant).

Use the best available algorithm for now, review later. Also review security
implications for each usage.

### State checksums

Calculate as the XOR of the checksum of each data item in the partition. This
algorithm is simple, relies on the security of the underlying algorithm, and
does not require ordering of data items.

Restricted to the partition since partitioning should allow all operations
without loading other partitions.


Cloning
----------

Support cloning by straight copy? Ideally yes, but it means there is no
opportunity to introduce a fresh "clone" identifier.


Branching?
--------------

Ideally I wouldn't support this. But is it required to be able to make a local
copy of remote history before merging?


Compaction
---------------

This operation reduces the size of the historical log by reducing essentially
to a few snapshots in history. Elements not mentioned in these snapshots will
be forgotten completely. The purpose is to allow user-controlled partial
deletion of history.


File extension
-----------------

`.pip` for snapshot files: short, peppy, self-contained. `.piplog` for commit
log files (which must correspond to a snapshot file).