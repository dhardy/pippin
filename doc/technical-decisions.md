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

Item identifiers: cannot change when contents change so cannot be a checksum.
For now, a user-defined 64-bit uint.

Commit identifiers: (a) commit checksum like git, (b) version number combined
with repository "clone" identifier or (c) version number plus checksum?
Possibly a 32-bit number (max parent number +1, loops), followed by 12 bytes of
some checksum?

Root pseudo-commit: give it a special identifier, all zeros?


Checksums
----------------------

Algorithm: SHA 256 or SHA-3 256? This could be selectable when creating an
empty repository and possibly when creating a new partition.

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
