<!-- This Source Code Form is subject to the terms of the Mozilla Public
   - License, v. 2.0. If a copy of the MPL was not distributed with this
   - file, You can obtain one at http://mozilla.org/MPL/2.0/. -->

Reasoning behind decisions
=================

Note that this is more a dumping ground for old notes (those still deemed
relevant) than it is an organised rationale for decisions behind the project.
But hopefully the same meaning can be extracted.


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

### Uniqueness within partitions

[Assuming use of `u64` for identifiers.]

Element identifiers need to be unique within the repository, however
determining uniqueness is best done per partition, thus identifiers should
have two parts: partition identifier and within-partition element identifier.

A single 64-bit unsigned number could be used, perhaps as a u32 identifying the
partition and another u32 unique within the partition (~4e9 elements per
partition), or using the first 24 bits as the partition identifier (~16e6
partitions) and the other 40 within the partition (~1e12).

New partition identifiers must be assigned whenever one identifier would be
split between multiple new partitions. I cannot see a way around doing this
without checking all partition identifiers that does not place unacceptable
restrictions on the availablility of new partition identifiers. This can
perhaps be mitigated by assigning "partition" identifiers on a more finely-
grained basis than necessary, e.g. to each possible classification whenever
assigning.

Identifiers could be suggested by the user, subject to verification of
uniqueness.


Finding elements given the identifier
-----------------------------------

The chosen identifier allocation strategy, based on the above, is to use the
current partition identifier plus a number unique within the partition, and not
to relabel when repartitioning since this is not required for uniqueness.

But, presented with an element identifier, how can we find the element?

There are two issues to deal with: repartitioning (mainly division into child
partitions) and reclassification (because the element changed).

### Check all partitions

The naive strategy is just to check each partition, starting with loaded ones.
For loaded partitions it's fairly fast since each has a hash-map of elements;
for unloaded partitions it's rediculously slow. There is a fair chance that the
partition part of the identifier gives the correct partition, but some
use-cases (e.g. accepting all new data into an "in-tray", then moving) will
mean this is mostly not the case.

### Relabel on repartitioning

Use the partition part of the identifier to find the partition. Alternatively,
attach an additional identifier describing the partition. Both methods require
adjusting the identifier when repartitioning and reclassifying; the advantage
of using an additional identifier for the partition is that the first part
would still be correct if an external reference to the element was not updated,
allowing the slow check-all-partitions strategy to find it again.

Invalidating externally held element identifiers on repartitioning is not
desirable, nor is having to make identifiers larger.

### Remember partition history

Use the partition part of the identifier to find the partition. If this
partition has been closed (repartitioning), use stored historical information
to find out which partitions it might now be in.

This is better at handling repartitioning than the naive strategy, but still
poor, and useless for reclassification of elements. New references to old
elements might still require loading more partitions to find the element on
each load of the database.

### Multiple names / redundant renaming

When repartitioning, give all elements new names: update the partition part to
the new partition identifier, *but* remember their old names too.

Where a partition has been divided, child partitions can be checked or the
parent could have a list of where each element went. Where elements are
reclassified, the parent partition would have to store each element's new
identifier (note that the second part of the identifier might need to be
changed too to avoid a clash).

External references *should* be updated (for faster access) but will work
either way.

A major disadvantage of this approach is that where reclassification is common
some partitions could end up with huge tables describing renaming and would not
be able to drop information from this table ever. Further, identifiers of moved
elements could not be re-used.

#### Variant: remember parent partitions

Don't remember old names of each element, just remember old partition
identifiers. On any look-up, if the partition identifier is that of a closed
parent partition, then for each child partition, replace the identifier with
the child partition identifier and check that partition.

This should work for repartitioning in most cases, but has two corner-cases:
(1) fabricated
element identifiers using an old partition identifier could potentially match
multiple elements, and (2) if partitions were to be (re)combined, some element
identifiers might collide and need to be reassigned, and to properly handle
this another look-up table would need to be consulted to track the renames.

Unfortunately, all reclassifications must still be remembered, by the source
partition to allow fast look-ups, and optionally by the target partition
(possibly only to support naive search if the source partition forgets).
Source partitions could forget about a move if (a) the element is deleted and
the source is notified (either by the target remembering the move or by some
kind of slow optimisation proceedure) and/or (b) after some period of time (if
naive searches are still possible or this kind of data-loss is acceptable to
the application).

The main problem with the source partition having to remember all moves is that
it could be problematic for this use case: new elements arrive via an "in-tray"
(a temporary initial classification) and are later classified properly (i.e.
moved). This partition must remember all moves, and if ten or one hundred
million elements are added, a table of this many items must be loaded every
time the partition is loaded. There is a work-around for this case: tell the
system not to remember moves for very long on this partition (remembering them 
would be a good idea for synchronisation however).


Checksums
---------------

### Goals

Checksums should be added such that (a) data corruption can be detected, (b)
replay of log-entries can be verified and (c) to protect against deliberate
checksum falsification of checksum/identifier ("birthday paradox" attacks),
thus providing a short and secure "state identifier".

State checksums should provide a mechanism to identify a commit/state of a
partition and validate its reconstruction (including against delibarate
manipulations). Additionally, given a state and a commit on that state,
calculation of the state sum of the result of applying the commit should be
straightforward (without having to consider elements not changed by the
commit).

### Choice of algorithm

We use checksums in two different ways:

1.  File corruption detection: for this, the algorithm must be fixed (unless
    file headers are re-read *after* finding out which algorithm is in use).
2.  Validating elements and partition state reproduction, identifying states

For the first use, security is not important and usage is small (once per
header, per snapshot and per commit); therefore choice is not very important;
for simplicity we use the same algorithm as for the second use-case.

For the second use, security is important (if secure validation of data is
desired). The SHA-2 and SHA-3 family of algorithms appear to be a good match
for our use case, but BLAKE2b is faster and is according to its authors "at
least as secure as the latest standard SHA-3" (it is derived from an SHA-3
competition finalist).

The current checksum algorithm is BLAKE2b configured for 256 bits output.
BLAKE2b was selected over other variants since target usage is on 64-bit CPUs,
and on multicore CPUs there may be better ways to process in parallel;
that said BLAKE2bp should probably be tested and properly considered.

#### Older notes

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

Usage is restricted to a single partition since partitioning should allow all
operations without loading other partitions.

#### Original approach

Element sums are simply checksums of element data. State checksums are just all
element sums combined via XOR (in any order since the operation is
associative and commutative).

This is convenient but has a few issues:

1.  if a commit simply renames elements, the state sum stays the same even
    though for many purposes the data is not the same
2.  collision attacks are made easier since a mischievous element whose sum
    matches *any* element can be inserted at *any* position simply by replacing
    the element with matching sum then renaming
3.  commits reverting to a previous state and merges equivalent to one parent
    have a colliding state sum, which undermines usage as an identifier

Number (2) is not really an issue, since in a partition with a million elements
it reduces complexity by 20 bits (2^20 is approximately one million). The
maximum partition size is 2^24 elements. This reduces complexity of a collision
attack from 256 bits to 232 bits at best. In comparison, the widely-applicable
"birthday paradox" attack reduces complexity by a factor of one half (to 128
bits).

#### New approach

Use the element's identifier in the element sum; the easiest way to do this
without having to further question security of the sum is to take the checksum
of the identifier and data in a single sequence.

State meta-data (including parent identifier) is in some ways important and
should also be validated by the sum. Further, including the parent sum(s) in
the state sum means that a revert commit or no-op merge cannot have the same
state sum as a previous state.

XOR is still used to combine sums, effectively making a state a set of named
elements. This is convenient for calculating sums resulting from patches. I see
no obvious security issues with this (since all inputs are secure checksums and
no other operations are used on sums).
