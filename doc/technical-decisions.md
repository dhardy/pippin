<!-- This Source Code Form is subject to the terms of the Mozilla Public
   - License, v. 2.0. If a copy of the MPL was not distributed with this
   - file, You can obtain one at http://mozilla.org/MPL/2.0/. -->

Technical design of sync-sets
====================

This mostly houses decisions/thoughts on details not organised under another
document.

Identifiers
------------------

### Element checksum

This is just a checksum of the element's identifier (a u64 encoded to bytes as
big-endian) followed by its data.
The current checksum algorithm is BLAKE2b configured for 256 bits output.

Note: SHA2 256 was the default, but BLAKE2b is faster and is according to its
authors "at least as secure as the latest standard SHA-3". BLAKE2b was selected
over other variants since target usage is on 64-bit CPUs, and on multicore CPUs
there may be better ways to process in parallel; that said BLAKE2bp should
probably be tested and properly considered.

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

*   each parent's statesum, as ordered by the commit
*   the commit's timestamp (UNIX time as big-endian i64)
*   the bytes `CNUM`
*   the commit number (big-endian u32)
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
on 64-bit hardware. [BLAKE2](https://blake2.net/) is faster than SHA-256
(likely also SHA-512) on 64-bit hardware, is apparently "at least as secure as
SHA-3" and can generate a digest of any size (up to 64 bytes).

My amateur thoughts on this:

1.  The "state sum" thing is used for identifying commits so there *may* be
    some security issues with choosing a weak checksum; the other uses of the
    checksum are really only for detecting accidental corruption of data so are
    not important for security considerations.
2.  If a "good" and a "malicious" element can be generated with the same
    checksum there may be some exploits, assuming commits are fetched from a
    third party somehow, however currently it is impossible to say for certain.
3.  16 bytes has 2^(8*16) ~= 10^38 possiblities (one hundred million million
    million million million million values), so it seems unlikely that anyone
    could brute-force calculate an intentional clash with a given sum,
    *assuming there are no further weaknesses in the algorithm*. Note that the
    birthday paradox means that you would expect a brute-force attack to find a
    collision after 2^(8*16/2) = 2^64 ~= 2*10^19 attempts, or a good/bad pair
    after ~ 4*10^19 hash calculations, which may be computationally feasible.
3.  SHA-1 uses 160 bits (20 bytes), with theoretical attacks reducing an attack
    to around 2^60 hash calculations, and is considered insecure, with one
    demonstrated collision to date.

Therefore using a 16-byte checksum for state sums seems like it would be
sufficient to withstand casual attacks but not necessarily serious ones.
SHA-256 uses 32 bytes and is generally considered secure. If the cost of using
32 bytes per object does not turn out to be too significant, we should probably
not use less.

As to costs, one million elements with 32-bytes each is 32 MB. If the elements
average 400 bytes (a "paragraph") then the checksum is less than 10% overhead,
however if elements are mostly very short (e.g. 10 bytes) then the overhead is
proportionally large and might be significant. Obviously this depends on the
application.

Ideally we would let the user choose the checksum length; failing
this 32 bytes does not seem like a bad default.

References:
[some advice on Stack Overflow](http://stackoverflow.com/a/5003438/314345),
[another comment](http://stackoverflow.com/a/23444843/314345),
[Birthday paradox / attack](https://en.wikipedia.org/wiki/Birthday_attack),
[SHA-1 attacks](https://en.wikipedia.org/wiki/SHA-1#Attacks).

### State checksums

Calculate as the XOR of the checksum of each data item in the partition. This
algorithm is simple, relies on the security of the underlying algorithm, and
does not require ordering of data items.

Restricted to the partition since partitioning should allow all operations
without loading other partitions.


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
