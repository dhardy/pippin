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


Checksums
---------------

Checksums should be added such that (a) data corruption can be detected and (b)
replay of log-entries can be verified.

State checksums: a deterministic method of producing a single checksum on the
full current state of a partition (data of all entries). Question: combine entry
data in a commutative method or not? For: easier parallelisation of calculation,
should be possible to calculate the new checksum from the old and a knowledge
of changed entries; against: nothing?
