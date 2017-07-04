<!-- This Source Code Form is subject to the terms of the Mozilla Public
   - License, v. 2.0. If a copy of the MPL was not distributed with this
   - file, You can obtain one at http://mozilla.org/MPL/2.0/. -->
<head><meta charset="utf-8"/></head>

Classification Mathematics
===========

## Definitions

Element space (type): `E`  
Elements: set `A` subset of `E`

Partition: `Ti` subset of `A`  
All partitions: `T` = set of all `Ti`; union of all `Ti` = `A`  
Partition identifiers `i`: just a number? A path or name?

Properties: set of property functions `P = {P1, ... Pn}`  
Property: function `Pi: E → Di`  
Property domain: `Di` is set of all values of a property (may include "not applicable" or similar)  
Thus: `P: E → D` where `D = D1×D2×...×Dn`

Classification: `Ci` subset of `D`  
Thus `Ti = {e in A | P(e) in Ci}`  
All classifications: `C = {Ci}`

## Questions

Should the program check that `Ci` never overlap on start?
Should it check instead that for each `e in E` used, `∃! i | P(e) in Ci`?

Should the program check that `union Ci` covers `D`?

Should classification `Ci` be a fixed property of each partition `Ti`, or should it be allowed to
change (thus being a property of the partition state)?
It should at least be possible for a commit to mark a partition as "being closed" and for a
snapshot to record the new state; possibly this can be extended to allow arbitrary reclassification
by a "marker commit" + new snapshot.


## Requirements

### Allowing changes to the set of properties `P`

What if `P` doesn't agree between partitions?  
→ `P` is a property of the whole repo, specified once by the user. Partitions use a subset.

What if a user changes `Pi(e)` for some `Pi`, `e`?  
→ User must notify that some partitions may now contain incorrect elements  
→ It may make sense to adjust `C` to minimise relocation work
→ Search and filtering may not work until all elements have been moved

What if a new property `Pi` is added?  
→ No action needed since existing partitions ignore unused properties

What if a property `Pi` is removed?  
→ If no partition uses it, no problem  
→ If a partition `Ti` depends on it, its `Ci` must be updated; it may be necessary to replace usage
    with a new Pi or to merge with another partition
→ Search and filtering may not work until all elements have been moved

What if a partition `Ti`'s `Ci` changes?  
→ If new `Ci` is superset of old, no reclassification needed internally (but elements may be added)  
→ Otherwise reclassification of elements is needed  
→ If new `Ci` is subset of old, no elements will be added

### Representing `T`, `P` and `C`

`T` may simply be a list of partitions by name or other identifier. It may be specified by the user
or discovered by reading file names from disk.

`P` must be specified by the user. It may be specified after reading some data if search and
insertion is not required prior to this (see below), so it could be dynamically set by the user
dependent on values read from a known element in a known partition.

Representation of `C` depends on (subsets of) `P` and `D`. These could be specified by some name or
other identifier, and computation aborted if relevant `Pi`, `Di` or members of `Di` are unknown.
Each `Ci` could be specified via:

*   a multidimensional matrix of booleans over `D` [fully flexible, may be large]
*   a range, subset or other set of rules for each applicable `Di` [has limitations on interaction
    between `Di`]
*   a single composed rule over `D`, using AND, OR, equality, inequality and/or membership rules
    [fully flexible, more complex to use]

## Bootstrap / discovery / limited-knowledge operation

Note that the specification of `C` depends on `T` and `P`. All three may be specified directly by
the user, or `T` discovered and the rest inferred then specified by the user, or `T` and `C` may be
discovered. Where the user specifies these directly the subsections below are irrelevant.

### Discovering `T`

The set of partitions may change over time. There is no requirement that partition `x` be available
or even known in order to access and modify partition `y`, although `x` may be required when
inserting or moving elements or possibly when creating a new partition `z`. Therefore it is not
required that this set, `T`, be known at start-up, but it is useful to have a way to discover `T`.

In the case that all data files are in a single directory or directory tree whose location is
given it is very easy to discover `T`, though some assurance that data files correspond to the same
repository may be useful.

### Specifying `P`

Property functions must be provided as Rust functions by the library user. There is no harm
in providing unused functions. Therefore there should not be any discovering to do, only specifying
by the user.

Data files could (and probably should) list identifiers for required property functions. This
could be used to load function code on demand and to fault-check missing properties.

### Discovering `C`

This can be divided into two subproblems: discovering `Ci` for the current partition `i`, and
discovering `Cj` for other partitions (or the whole of `C`).

Pippin is designed to work without any centralisation of repository configuration; the user may
add this, but Pippin should be able to discover and use repository files without it. This implies
that information about `C` must be distributed.

Each partition `i` may store information about its own partitioning (`Ci`), but information stored
about other partitions can never be complete and may be outdated. Partition `i` can be used without
knowing about other partitions, so long as the classifications `C` have no overlaps among "live"
partitions and each partition can mark itself "dead" or as matching a reduced classification
(i.e. partially or completely superseded by other partition(s)).

To find existing or insert new elements not classified as a member of a known partition,
the classification of (some) other partitions needs to be discovered.

Simplest option: read metadata from each partition discovered.  
→ Simple  
← Finding elements of unknown location is expected `O(n)` in the number of unknown partitions

Another option: store a description of the known subset of `T` in each partition's metadata.  
→ Not very complex  
← May result in large headers with lots of redundency  
← Allows multiple different specifications of `Ci` for some `i`, thus either each `Ci` must be
fixed or the active description updated when that partition is loaded  
← Finding elements of unknown location can be fast when most of `C` is known at the time metadata
is written, but still expected `O(n)` where `C` is mostly unknown during writing

Subset of above: store the list of partition numbers and names/location in each
partition's metadata, but no classification data.  
→ Accelerates discovery of `T`
← Does not help discover `C`

User is responsible for specifying `P`, `C` and `T`. Library can discover available partitions and inform
of missing or unused partitions but no more.  
→ Simple from library perspective; no issues with conflicting definitions of `C`  
→ More flexible for user  
← More work for user implementation  
← Doesn't support fully dynamic, distributed partitioning unless user can discover `C` from file
names; a compromise using a centralised description of `C` may work reasonably in practice.

### Operation without knowing full `T`, `P` and `C`

Clearly, only the known subset of `T` can be loaded. (This can be discovered from file names, and
does not require knowledge of `P` or `C`.) Extending `T` (by discovering other data sources or by
partition creation) is easy. Shrinking `T` (deleting partitions or making unavailable) is simple
enough but may also shrink the apparent set of elements `A`.

Loading partitions, listing elements, and finding elements by identifier within loaded partitions
is possible without `P` or `C`.

Inserting elements without sufficient `P` and `C` to find a suitable partition is impossible. As a
workaround, a trivial property "is new" and a partition for temporary elements (an "in tray")
could be used, with reclassification forced once `P` and `C` are available.

Extending `P` with new or newly known properties is easy. Removing properties from `P` is easy.
Operation without all properties may mean that membership `P(e) in Ci` is not computable, and
that not all filters are available for search.

Adjusting `C` would change insertion and search behaviour. If these features are not used before
`C` is defined, this is acceptable. Workarounds may be possible. If `C` does not match previous
definition(s) and reclassification is not carried out then search results may be incorrect; the
user must take responsibility for this.

This implies that the `Ci` for each partition `i` should not change; in other words, it should be
set when the partition is created and never altered.

-------

## User interface

### User assignment of partitions

*   User may desire to create some partitions immediately, even when sizes are small
*   Large partitions are an indicator that splitting may be useful, but not a requirement
*   Memory, disk & CPU performance is complex: optimal partitioning may not be obvious
*   Should the library take *any* responsibility for deciding partitioning or when to partition?

### Data source

A data provider can scan the disk for files and directly tell the user of known partitions.

The provider can then be connected to a repository and used to provide data access.

### Repository

Collects a bunch of partitions, allows filtering / searching, insertion, element list
functionality.

Repository must be told which partitions to load, but may auto-load known partitions.
Repository must be told `T`, `P` and `C`, but doesn't need to know all rules immediately.

User may use a known partition to store config data, or may store local config data in each
partition's metadata and detect partitions on disk.

API requirements:

*   specify data source [i.e. FS access, like SQLite's VFS]
*   specify property functions
*   create new repo
*   open/discover existing repo
*   split a partition into two or more, deprecating the old
*   (restore a deprecated partition, undoing a split or restore?)
*   mark a partition as needing reclassification

User requirements:

*   connection to storage (files)
*   specification of properties
*   specification of classifications when splitting [optional]
*   specification of new partition names [optional]
*   suggestion of partition to load for some given classification
    [sometimes guessable from names, to speed up finding data]


Questions:

*   Have a repository identifier? Allow it to change? Verify it when loading?
*   What should partition identifiers look like? Mapping rules for file names? Store in files?
*   How are properties specified?
*   How are classification rules specified?
*   Allow changing metadata?

---------

## Searching / filtering elements

Part of the point of adding properties is to allow the user to quickly find all elements matching
given criteria. But how can Pippin make searching fast?

Partitions not matching the given classification can be disregarded. If partitioning is similar to
the search criteria, this could significantly speed up searching. If partitions are small enough,
it could even make searching fast — though it may be more practical to use larger partitions to
reduce the number of files and memory overhead.

Caveat with partitioning: if a partition is assigned a range of property values but actually
only contains elements classified inside a much smaller range, then without additional information
the partition may match the search criteria but contain no matching elements. (The first part is
quite likely given naive partitioning and low-granularity properties, however it may still not be
a problem if search criteria are picked based on actual property outputs.)

Adding just a list of property output values (where few) or range to partitions would avoid the
above caveat without a lot of overhead (probably most commits would not affect it). This could also
aid smart automatic partitioning.

How about adding an index for each partition and property? If this is build in-memory as a
partition is loaded, and especially if built in relation to common search criteria, the indices
may significantly speed up searches at the cost of some extra memory usage. It is probably not
useful trying to store any indices in data files, since each new commit would have to patch the
index for each element modified, yet property functions are supposed to be fairly fast, so the
information would be redundant. An index stored in a snapshot might have some value.

-----

## Model

Repository and/or partition control should expose a set of "property" trait objects [user defined].

Partitions (and partition states?) should have a defined classification based on properties
[determined by partitioning tool].

Repositories (and their states?) should have a tool for finding a partition from an element, and
more tools for finding a set of partitions matching a set of properties somehow [lib code].

Repository control should have a "partitioning tool", provided as a trait object, which tries to
divide a partition by producing multiple sub-classifications [lib default or user code].

### Questions

Does classification information need to be in both the *partition* and the *partition state*?

Do property functions need to be accessible from *repository states* (required for automatic
partition selection on insertion)?

Should insertion on repo states and partition states behave differently: repos checking
classification while partitions don't?
