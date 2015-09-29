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

Most identifiers will be ASCII and right-padded to 8 or 16 bytes with space
(0x20) bytes, or they will be binary.

File format: 16 bytes: PIPPIN-space-YYYYMMDD-space (e.g. `PIPPIN 20150916 `). It is expected
that many versions get created but that few survive to the release stage.

Commit identifiers: use the commit checksum as an identifier like git etc.,
*but* this is only to identify the commit (as a patch between two states of the
repository). Use state checksums to identify repository states.

Rationale: commit identification is important for merges. When however it comes
to deleting old history, commits will either be forgotten entirely or merged
into larger commits with new checksums. State checksums, however, will remain
the same (for those states which survive).

Initial state: give it a special identifier, all zeros (i.e. the result of XOR
combining zero element checksums).

Element identifiers will be 64-bit unsigned numbers unique to the
file/partition. There may be an API for suggesting identifiers but the library
will give final approval/disapproval. It may be necessary to change identifiers
in the case that partitions are joined. Rationale: uniqueness to the partition
is important, uniqueness beyond that is hard to determine when partitioning
means that not all elements are loaded.


Elements
------------

Elements have a 64-bit unique numeric identifier and a checksum.

TBD: rest of data (user-defined format?).


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

User-defined functions map from elements to values. TBD: domain of the functions
(multiple options, generic, only a fixed number?).

Priority of the classifier functions is user defined.

### Identifiers

In memory, partition identifiers will have no particular meaning and may not
have the same value the next time the repository is loaded. Users may simply
request to list or search entries under some restriction of classifier values,
or may enquire about partitions to e.g. only read from the partition of most
recently dated entries (where date is a classifier).

On disk, partition names may also be arbitrary, but to aid users names may be
suggested by the program using the library.


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
