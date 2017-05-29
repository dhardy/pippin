<!-- This Source Code Form is subject to the terms of the Mozilla Public
   - License, v. 2.0. If a copy of the MPL was not distributed with this
   - file, You can obtain one at http://mozilla.org/MPL/2.0/. -->
<head><meta charset="utf-8"/></head>

Classification Mathematics
===========

## Definitions

Element space (type): `E`  
Elements: set `A` subset of `E`

Partition: `Pi` subset of `A`  
All partitions: `P` = set of all `Pi`; union of all `Pi` = `A`  
Partition identifiers `i`: just a number? A path or name?

Classifiers: set of classifiers `C = {C1, ... Cn}`  
Classifier: function `Ci: E → Di`  
Classifier domain: `Di` is set of all values of a classifier (may include "not applicable" or similar)  
Thus: `C: E → D` where `D = D1×D2×...×Dn`

Classification: `Li` subset of `D`  
Thus `Pi = {e in A | C(e) in Li}`  
All classifications: `L = {Li}`

## Requirements

### Allowing changes to the set of classifiers `C`

What if `C` doesn't agree between partitions?  
→ Treat non-identical classifiers as distinct; build a larger `C` of all classifiers  
→ Expand each partition's `Li` to new larger `D`  
→ When saving partition metainfo, use an "optimised form" of `C` and `Li` omitting all irrelevant `Ci`

What if a user changes `Ci(e)` for some `Ci`, `e`?  
→ User must notify that some partitions may now contain incorrect elements  
→ Search and filtering may not work until all elements have been moved

What if a user changes `Di` for some `Ci`?  
→ Some `Li` may no longer make sense and need correction  
→ Some `Ci(e)` may change (see above)

What if a new classifier `Ci` is added?  
→ No action needed since existing partitions ignore unused classifiers

What if a classifier `Ci` is removed?  
→ If no partition uses it, no problem  
→ If a partition `Pi` depends on it, the corresponding `Li` must be corrected and elements reclassified  
→ Filtering by other classifiers may fail until elements are reclassified

What if a partition `Pi`'s `Li` changes?  
→ If new `Li` is superset of old, no reclassification needed internally (but elements may be added)  
→ Otherwise reclassification of elements is needed  
→ If new `Li` is subset of old, no elements will be added

### Representing `P`, `C` and `L`

`P` may simply be a list of partitions by name or other identifier. It may be specified by the user
or discovered by reading file names from disk.

`C` must be specified by the user. It may be specified after reading some data if search and
insertion is not required prior to this (see below), so it could be dynamically set by the user
dependent on values read from a known element in a known partition.

Representation of `L` depends on (subsets of) `C` and `D`. These could be specified by some name or
other identifier, and computation aborted if relevant `Ci`, `Di` or members of `Di` are unknown.
Each `Li` could be specified via:

*   a multidimensional matrix of booleans over `D` [fully flexible, may be large]
*   a range, subset or other set of rules for each applicable `Di` [has limitations on interaction
    between `Di`]
*   a single composed rule over `D`, using AND, OR, equality, inequality and/or membership rules
    [fully flexible, more complex to use]

## Bootstrap / discovery / limited-knowledge operation

### Discovering `P`, `C` and `L`

Simplest option: read metadata from each partition available.  
→ Simple  
← Finding elements of unknown location is `O(n)` in number of unknown partitions  
← `Li` for distinct `i` could overlap; union of `Li` over `i` may not cover `D`

Another option: store description of known subset of `P` in each partition's metadata.  
→ Not very complex  
← May result in large headers with lots of redundency  
← Allows multiple different specifications of `Li` for some `i` [versioning helps, but fundamentally
    still allows disagreements]  
← Finding elements of unknown location is faster but still worst-case `O(n)`

User is responsible for specifying `C`, `L` and `P`. Library can discover available partitions and inform
of missing or unused partitions but no more.  
→ Simple from library perspective; no issues with conflicting definitions of `L`  
→ More flexible for user  
← More work for user implementation  
← Tricky for user to store configuration data within the repository?

### Operation without knowing full `P`, `C` and `L`

Clearly, only the known subset of `P` can be loaded. (This can be discovered from file names, and
does not require knowledge of `C` or `L`.) Extending `P` (by discovering other data sources or by
partition creation) is easy. Shrinking `P` (deleting partitions or making unavailable) is simple
enough but may also shrink the apparent set of elements `A`.

Loading partitions, listing elements, and finding elements by identifier within loaded partitions
is possible without `C` or `L`.

Inserting elements without sufficient `C` and `L` to find a suitable partition is impossible. As a
workaround, a dummy classifier "is new" and a partition for temporary elements (an "in tray")
could be used, with reclassification forced once `C` and `L` are available.

Extending `C` with new or newly known classifiers is easy. Removing classifiers from `C` is easy.
Operation without all classifiers may mean that membership `C(e) in Li` is not computable, and
that not all filters are available for search.

Adjusting `L` would change insertion and search behaviour. If these features are not used before
`L` is defined, this is acceptable. Workarounds may be possible. If `L` does not match previous
definition(s) and reclassification is not carried out then search results may be incorrect; the
user must take responsibility for this.

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
Repository must be told `P`, `C` and `L`, but doesn't need to know all rules immediately.

User may use a known partition to store config data, or may store local config data in each
partition's metadata and detect partitions on disk.

API requirements:

*   create new
*   attach data source [allow multiple?]
*   load a partition by a given identifier
*   unload a partition by identifier
*   add classifier, with domain
*   remove classifier
*   add partition
*   add an initial state to a partition
*   set a partition's classification rules
*   mark a partition as needing reclassification
*   save all data changes
*   do maintenance jobs

Questions:

*   Have a repository identifier? Allow it to change? Verify it when loading?
*   What should partition identifiers look like? Mapping rules for file names? Store in files?
*   How are classifiers specified?
*   How are classification rules specified?
*   Allow changing metadata?
