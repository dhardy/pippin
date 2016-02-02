<!-- This Source Code Form is subject to the terms of the Mozilla Public
   - License, v. 2.0. If a copy of the MPL was not distributed with this
   - file, You can obtain one at http://mozilla.org/MPL/2.0/. -->

Technical design of sync-sets
====================

This mostly houses decisions/thoughts on details not organised under another
document.

Identifiers
------------------

### State sums

These are checksums which (1) identify a state reached by a snapshot
or commit, and (2) validate data in a recreated state. They can be used to find
snapshots and commits, but there may be multiple snapshots and commits for a
single state-sum.

Note that if a commit ever reverts to a previous state, this will create a loop
in the history graph, rendering the reverting commit redundant and (unless
there is a snapshot within the loop) useless. Code should deal with this.

Initial state: give it a special identifier, all zeros (i.e. the result of XOR
combining zero element checksums).

### Commit identifiers

The following has never been implemented. Should we add these?

Commit identifiers: use the commit checksum as an identifier like git etc.
This may not be required, since normally one would only request a state and not
care how it is reproduced.

Rationale: commit identification is important for merges. When however it comes
to deleting old history, commits will either be forgotten entirely or merged
into larger commits with new checksums. State checksums, however, will remain
the same (for those states which survive).

### Displaying checksum identifiers

Printing state sums and commit checksums: use base 36 (0-9, a-z; i.e.
hexadecimal extended to the end of the alphabet). Always use lower-case letters
to avoid confusion between zero (`0`) and upper-case `O`. A 256-bit number
requires only 50 characters in base-36 (or 64 in base 16). Like git, accept
abbreviated sums so long as these are unique within known history.

### Element and partition identifiers

Element identifiers are a unique `u64` (unsigned 64-bit number). We don't care
much what they are so long as they are unique.

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
