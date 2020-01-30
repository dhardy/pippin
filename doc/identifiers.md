<!-- This Source Code Form is subject to the terms of the Mozilla Public
   - License, v. 2.0. If a copy of the MPL was not distributed with this
   - file, You can obtain one at http://mozilla.org/MPL/2.0/. -->

Identifiers
===========


Partition identifiers
--------------

Currently these are 40-bit numbers.



Element identifiers
------------

Currently these are partition-dependent 64-bit numbers (24-bits within the partition).

Required properties:

*   Unique — users do not want to have to deal with the alternative;
    *probably unique* identifiers may be sufficient *if reasonably secure*
*   Allow data mutation
*   Invariant — current design violates this by changing partition-part when moving!

Options:

*   Small numbers, strings, or a custom type?
*   No identifiers — just objects with properties on them (makes key uniqueness harder?)
*   Large random number or hash and assume uniqueness or handle clashes

Notes with respect to partitions:

*   Including the current partition's identifier makes the element identifier variant(!)
*   Including the originator's identifier helps with uniqueness but is not super useful
*   Uniform distribution of identifiers over a large domain which is then partitioned *does* allow
    invariant (probably-unique) identifiers and partitioning, but is partitioning with random
    (uncorrelated) allocation actually useful?


### Identifier uniqueness

*   Testing uniqueness within one "partition" is quite feasible, but still requires reading a table
    of all element identifiers or traversing a table stored in the file
*   Testing uniqueness across multiple "partitions" is harder, and defeats most of the point of
    using partitions
*   Including the current partition's identifier in an element identifier ensures uniqueness, but
    if the element can move to another partition (a) it could be given a new identifier (variant!)
    or (b) it could keep the same identifier, implying the identifier is not so helpful in locating
    the element.

Hence partitions are probably only useful if fixed (no dynamic repartitioning) and elements can't
move: only possible if user-defined and tailored to a specific use-case (e.g. a partition per data
category — but in this case there is little reason to group the partitions under a single
"repository", especially since transactions/commits cannot span multiple partitions).


### Identifierless data

Proposition: elements have no intrinsic key.

Insertion could be possible with no key, but elements would be immutable and not deletable (since
there's no way to reference them to say that they changed, given grow-only storage).

A key could be derived via a property function (e.g. hash of whole element or an embedded value),
and a table built mapping keys to objects, the objects being stored in-place or via pointer. The
table could be saved in files as a b-tree whose nodes can be replaced in commits, thus making the
tree mutable on top of grow-only storage. Allows modification and deletion.

Multiple lookup tables (each using a different property function) would be possible. But how
would the refered elements remain synchronised? In-line storage of elements would not be possible,
since updating an element found by one property table must also update it as found by other
tables. Further, improper updates could update one property table but not another. Actually, if
one table is a primary key, objects could be stored inline in that table.


### Identifier prefixes

It may be useful to users to let them specify a common prefix for all elements in the repository:
it allows the user to have a common identifier across repositories without appending another
piece of data.
