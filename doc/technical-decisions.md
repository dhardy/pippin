<!-- This Source Code Form is subject to the terms of the Mozilla Public
   - License, v. 2.0. If a copy of the MPL was not distributed with this
   - file, You can obtain one at http://mozilla.org/MPL/2.0/. -->

Technical design of sync-sets
====================

This mostly houses decisions/thoughts on details not organised under another
document.

Identifiers
------------------

### Element sum

This is just a checksum of the element's identifier (a u64 encoded to bytes as
big-endian) followed by its data.
The current checksum algorithm is BLAKE2b configured for 256 bits output.

Note: the checksum output size can easily be adjusted (`BYTES` in
`src/detail/sum.rs` — other values should work but are not often tested),
but doing so requries building new binaries and renders old data files
incompatible. 256 bits appears to be a common choice and a good compromise
between size and security.


### State sums

A *state sum* is a checksum on a *state* (a commit or snapshot) of a partition.
This serves both as an identifier and a means to securely identify the
partition's data (the set of elements) and metadata (parent state(s), author,
timestamp, moved elements, maybe more).

A checksum of the metadata is computed from the following stream of data:

*   the partition identifier (u64 but lower 24 bits are zero, big-endian)
*   the bytes `CNUM`
*   the commit number (big-endian u32)
*   the commit's timestamp (UNIX time as big-endian i64)
*   each parent's statesum, as ordered by the commit
*   if present, the extra metadata byte-stream (without padding)

The state sum is the metadata checksum and element sums combined via bit-wise
exclusive-or (XOR) operator (in any order).

A new partition should be started with a blank state: no parents, no elements,
commit number 0; however the timestamp should be set to the time of
initialisation and extra metadata can be included (e.g. comments or author).

### Displaying checksum identifiers

Printing state sums and commit checksums: use base 16 (0-9, A-F) — hexadecimal.
Optionally put a space between pairs of characters.

Like git, accept abbreviated sums as identifiers so long as these are unique
within known history.

### Element and partition identifiers

Element identifiers are a unique `u64` (unsigned 64-bit number) and also serve
to identify the element's partition.

The first 40 bits (high part) of the number is allocated to a partition
identifier, and the low 24 bits (more than 16 million numbers) are allocated
to an element identifier within the partition.

(Example: a partition might use 123 as the high part. A new element can be
assigned any 32-bit number unique within the partition, e.g. 5. The element
identifier would then be 123 × (2^40) + 5.)

#### Element identifier assignment

The low 24 bits can be assigned in any way such that they are unique within the
partition. A good strategy might be to randomly sample a number distributed
evenly throughout the range, then increment by one until a unique number is
found.

#### Partition identifier assignment

Each partition is assigned a range of partition identifiers; the initial
"partition" in a repository gets the whole available range.

The partition uses the smallest number within its range when assigning new
element identifiers.

When a partition is partitioned, the range is first adjusted to exclude the
smallest number (which has already been used), then is divided equally between
the new partitions, which get (roughly) equally-sized non-overlapping ranges.
The old partition will not be used anymore (unless possibly the new partitions
are merged back together).

Should a partition run out of new numbers for partitioning, another strategy is
possible: find a little-used partition with more numbers than it needs, and
steal some of its range. The details for this are yet to be defined.

The range (0, 0) is a special case used in the library when loading data. This
range should never be used otherwise.


### Snapshot and commit log file names

See *identifiers* section under *partitions*, below.

### Repository name

A repository must have a name specified when created (UTF-8; not more than 16
bytes long). This serves (1) as an identifier to check that partitions come
from the same repo, and (2) as a user description of the repo. It is visible
near the top of each repository file in plain text (assuming no compression).


Cloning
----------

Support cloning by straight copy of data files? Ideally yes, but it means there
is no opportunity to introduce a fresh "clone" identifier.


Branching?
--------------

Pulling in commits from a remote copy (or possibly even committing locally) can
result in multiple *tip* states which then require merging. Unlike git, such
states are found without needing to keep track of *branches*. The compromise is
that a partition cannot be used until merging is complete (not even retrieving
elements).


Compaction
---------------

This operation reduces the size of the historical log by reducing essentially
to a few snapshots in history. Elements not mentioned in these snapshots will
be forgotten completely. The purpose is to allow user-controlled partial
deletion of history.
