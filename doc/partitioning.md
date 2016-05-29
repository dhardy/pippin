<!-- This Source Code Form is subject to the terms of the Mozilla Public
   - License, v. 2.0. If a copy of the MPL was not distributed with this
   - file, You can obtain one at http://mozilla.org/MPL/2.0/. -->

Partitioning
=======

Introduction
---------

A *partition* is defined from the API point of view: a subset of elements which
are loaded and unloaded simultaneously. For ease of notation, the 'container'
storing all data in an undivided repository is also considered a 'partition'.
The partition-file relationship is 1-many: a partition uses multiple files to
store the history of its data.

*Partitioning* of the repository into partitions is neither user-defined nor
fixed for the lifespan of the repository, but the user does get a lot of
control over how data is partitioned.

When a partition is large, it may be split into multiple child partitions by
(a) creating the "child" partitions and copying element data there, and (b)
marking the original partition as closed. In theory, the reverse is also
possible: moving data back to a "parent" partition and closing the children.
These processes are known as *repartitioning*.

Each partition is separate to the degree that it is not guaranteed to know the
details of other partitions — it stores some details, but these may be
obsolete. There is no *master index* and there is no requirement for such a
thing.

Each partition has a number and a *base-name* (the common path relative to the
partition root and file-name prefix of all its files).


Meta-data
---------------

We can store some data about partitioning in the partitions themselves. For
all known partitions, we can store:

*   the classification data for the partition
*   the "partition number"
*   the base-name of partition files on disk
*   the status (whether still active or split into child partitions)

This information may be out-of-date for some partitions, but provided we never
*change* a partition but only sub-divide into *new* partitions, it's quite easy
to spot what is out of date: either there will be no files on disk, or when
loaded, the files for that partition will say that it has been closed.

In memory, we use the most up-to-date information available (with some simple
mechanism for solving conflicts about information on unloaded partitions
received from loading other partitions, e.g. using a version number attached to
the partition info). When creating new partitions, we write all we know.


Discovering partitioning
------------------

Each partition must record at least what classification rules and partitioning
apply to its own elements. It cannot be enforced that all partitions know all
partitioning since this would make any repartitioning expensive.

Option 1) all snapshots include all partitioning rules as of when their
snapshots are created. Problem: extra work might be necessary when creating
snapshots and new partitions, and might not even be doable (if not all
partitions are available).

Option 2) all snapshots specify only their own partitioning. This has two
issues: (a) locating a required partition requires checking all partitions
until the required one is found, and (b) this provides no way of checking for
partitioning issues (gaps, overlaps) without reading all partitions.

Option 3) all snapshots include partitioning rules known (without doing extra
work) at the time they are created. This does not solve the second issue with
the previous option (checking for partitioning issues).

Option 4) all snapshots specify their partitioning encoded in their filename.
This is difficult to do properly when classifications are not fixed from when
the repository is created. It also requires either meaningless code names or
risks very long file names.

### Discovering partition meta-info

Assuming a set of files are identified as beloning to the same partition (from
their directory, name prefix or whatever): *how are their stati discovered?*

Option 1: sequential numbering of snapshots. This is fine except that it does
not allow changes from a cloned repository to be merged simply by dumping the
files in (unless "clone identifiers" are used).

Option 2: read all snapshots (at least their headers). Slow.

Option 3: remember snapshot correspondances. Feasible but using a "central"
file to remember this stuff is not desirable.

Option 4: each snapshot lists in its header all those snapshots which are its
ancestors. If the system can guess which snapshot is probably the latest (e.g.
from a sequential number), it can discover all relationships without reading
many files (in most cases). Seems okay, though it could require opening some
files multiple times.

#### Naming snapshot files

In light of this, it might be best if snapshot files have the form
`BASE-NUM-SUM.pip` where `BASE` is the basename (common within the partition),
`NUM` is a sequential number, and `SUM` is part of the snapshot state-sum. This
should make sure states in a "branched" repository have different names.

#### Commit logs

These should have the same `BASE-NUM-SUM` part as the corresponding snapshot
file. They should then have some unique identifier.

Option 1: a number.

Option 2: the state-sum of the latest state. This requires writing from scratch
or renaming on every new commit, which isn't ideal.

### Discovering partition files

When some kind of `RepoIO` is created, *some* checking of available partitions
and possibly files is required. Of the options below, 1 and 2 appear most
sensible.

#### Option 1: discover all paths on creation and remember all

+ everything available early

#### Option 2: Discover partitions available on creation, parition files when a `PartIO` is created

+ partitions known early
+ partition files known early enough for most purposes
- can't easily get stats of all partition files

#### Option 3 Discover partitions available on creation, partition files when first loaded

+ partitions known early
- can't easily get stats of all partition files

What is the point over option 2 though? Maybe that the `PartIO` can be created
earlier without scanning all files.

#### Option 4: Discover partitions available on creation, partition files dynamically (on demand)

+ partitions known early
+ "live" view of files on disk without rescanning
- getting file stats more difficult
- loading the tip is more difficult
- no easy way of spotting changes on the disk

#### Option 5: Discover at one partition, do the rest on demand

- partitioning information will not be available at first
- finding the correct partition will be slow the first time it is used
- partition discovery cannot sanely be "live" (always done from the disk)


When to partition
-------------------

The criteria for creating further partitions should be set by the user. Some
use-cases might be happy with large data-sets which are slow to load and
repartition but are still fast to read and fairly fast to write to; other cases
might be happier with many small partitions which can be loaded or
repartitioned nearly instantaneously.

The exact time of partitioning will likely still be determined by the library.


How to assign elements to partitions
------------------

User-defined classifiers are used to determine how to partition data.

The same classifiers can be used in some cases to search/filter elements.

Classification information is not required to always be available (in order to
support partial encryption of elements and server operations in the face of
incomplete information). If classification is not possible on an element, then
the server should not move it or sub-divide its partition. It will however need
a classification in order to support an insertion of a new element, so for some
usages of the library it might be sensible to have a designated "in-box"
partition or similar.

### Target use-cases

Partitioning of emails by (a) status (inbox, outbox, archived), (b) date of
sending, (c) group of sender (by matching the from address against a glob, or
regex or list), (d) by size.

### Chosen solution: External classifiers

We use compile-time polymorphism (otherwise known as templating or "generics")
to support custom *element* types. This means that it is trivial to extract the
underlying element type for use with a custom *classifier*.

Our classifier can thus be an external entity which can access details of
elements in whichever way the user of the library sees fit and returns a
classification into categories, whether the properties involved are integers,
strings or whatever else.

To allow the Pippin library to request sub-division of a category, all we need
is a classifier which returns the *enumeration* of an element's category and,
on request, can subdivide a designated category into two or more sub-categories
(or fail, if no more classification is possible).

This leaves two problems: (1) the user must write quite a bit of code to deal
with classification, and (2) the user-defined categories must be preserved. We
cannot completely solve (1) while leaving the user in control, but can provide
templates, and besides, classification is of little importance for small
databases. To solve (2), the provided classifier should be able to serialise
required details of its decision logic for each partition, so that this can be
stored in partition meta-data and be updated, should more up-to-date
information on some partition be found.

### Changing classifiers

If a user wishes to add, remove or change the priority of classifiers, this may
affect where existing elements are partitioned.

Issue 1: this might necessitate moving many elements, thus being expensive.
This is acceptable so long as the user is warned before committing to the
operation.

Issue 2: doing this when there are multiple partitions will result in not all
partitions advertising the same classification during the move. If interrupted,
move rules should ensure that elements are neither lost nor duplicated, but it
not be obvious how to recover. It may not even be obvious at first, however if
partitioning is not disjoint or not covering it will be noticed. Ideally
however the issue should be noticed the next time any partition is loaded and
offer to fix this immediately but be able to continue should the user decide
not to fix it immediately.


How to change partitions
-----------------------


### Repartioning

Problem: a repository is partitioned. Each partition is independent and may
not have knowledge of other partitions.

Partitioning is not fixed, but must always be disjoint (no overlaps) and
covering (no gaps).

The user wishes to be able to determine where some element should be placed
or can be found without analysing all partitions.

### Repartitioning strategy

1.  The new partition(s) are created empty (if not already existing)
2.  For each (source, target) partition pair, a commit is created on the target
    with the moved elements marked as a move with the source mentioned
3.  For each (source, target) pair, a commit is made to the source removing
    the elements moved, and marked as a move mentioning the target
4.  (Optional) a new snapshot is created on affected partitions after the move.
    In particular, for the source partition(s) where emptied or significantly
    smaller, this will speed up reads, and since the sources will likely see
    less on-going activity, new snapshots might not otherwise happen soon.

When reading the partition data, the elements are only considered moved if
*both* the source and target confirm the move, or the source is confirmed not
to include the elements.

Partial repartitioning is possible with the following restrictions:

*   If an element is searched for which could be in multiple partitions, all of
    those partitions should be checked.
*   If an element is saved where it could be placed in multiple partitions, it
    may be saved to any, but only to one.
*   Repartitioning *should* be completed when convenient, according to the
    classifier rules specified by the user and either the most recent
    partitioning strategy which corresponds to current classifier priority or
    a new partitioning strategy compliant with classifier priorities.

### More on repartitioning

Where elements move to a sub-partition, the original may stay with the same
name, only marking certain elements as moved. Alternately, all elements may be
moved to sub-partitions.

Where a partition is rendered obsolete, it could (a) remain (but with a new
empty snapshot) or (b) be closed with some special file. Maybe (a) is a form
of (b).

Where a partition is renamed, it could (a) not be renamed on the disk (breaking
path to partition name correlations), (b) be handled by moving files on the
disk (breaking historical name correlations, possibly dangerous), (c) be
handled by closing the old partition and moving all elements (expensive), or
(d) via some "link" and "rename marker" until the next snapshot is created.

#### Simplest solution

Partitions are given new names on the disk not correlating to partition path or
any other user-friendly naming method. Renaming paths thus does not move
partitions. All partitions are stored in the same directory. Partitions are
never removed, but left empty if no longer needed.

#### Allowing partition removal

(Obviously without deleting historical data.)

Option 1) use a repository-wide snapshot number. Whenever any new snapshot is
needed, update *all* partitions with a new snapshot file (in theory this could
just be a link to the old one), except for partitions which are deleted. Only
load from files with the latest snapshot number. This is not very robust.

Option 2) use an index file to track partitioning. This breaks the independance
of snapshots requirement.

Option 3) close the partition with a special file. The only advantage this has
over leaving the partition empty is that the file-name alone would indicate
that the partition is empty. OTOH a special file name could be used for any
empty snapshot file in any case.



Assigning partition identifiers
--------------------

A unique number is needed for each partition, as well as some *base-name* for
naming of its files on disk.

### Unique partition numbers.

How can we ensure that each partition gets a unique number?
These are needed to ensure elements get unique numbers.

This number should probably be assigned by the user-specified classifier as
the enumeration for the partition. This considers possible stragies which the
classifier might wish to use.

#### Bifurcation

Start at some number, e.g. 2^31 if we have a 32-bit unsigned int.

Every time a partition is split, create two new identifiers by setting the
most significant unset bit to 1 and in one case setting the next bit to 0.
Remove the previous number from use so that it is not used to reproduce these
numbers again.

Disadvantage: only 31 levels of splitting possible with 32 bits; less if
splitting into more than two new partitions at a time. Old numbers cannot be
reused.

#### Linear splitting

Adaptation of above, where each partition *remembers* the range it has
available, and divides this up among child partitions when partitioning. This
is considerably more efficient when splitting to more than two child
partitions.

It does have one drawback: if elements are not relabelled, then checking
uniqueness of new elements with the same partition number is not easy, so
probably additionally partitions must *remember* which numbers within their
range they are not able to use.

#### Redistribution

This could be added to either of the above, presumably when new partitions are
needed.

The idea is simple: seek out partitions with more numbers available than they
need, steal some (updating those partitions with their new number/range), and
assign these to new partitions.

There is some risk: if the program crashes at the wrong time, it might either
lose some numbers or double-assign them. Further, another process using the
library could *theoretically* divide the partition from which these numbers are
stolen at the same time, causing more problems.

### Base-name for partition files

The base-name could simply be the partition number formatted as a decimal. But
it could instead be provided by the user to allow more meaningful names for
partitions.

This base-name is an arbitrary Unicode string except that (a) it may contain
path separators (`/` on all operating systems), and (b) the part after the last
separator must be a valid file name stem and the rest must either be empty or
a valid (relative or absolute) path.

Sub-directories are allowed via `/` separators in paths, so for example a
base-name could be `archives/2013-2014`.

### File-names and base-name

Standard file extensions are `.pip` for snapshot files (it's short, peppy, and
self-contained), and `.piplog` for commit log files (which must correspond to a
snapshot file).

Given some `BASENAME` (see above),
snapshot files are named `BASENAME-ssN.pip` where `N` is the number of a
snapshot. The first non-empty snapshot is usually numbered `1`; subsequent
snapshots should be numbered one more than the largest number in use (not a
hard requirement). The convention for new partitions is that an empty snapshot
with number `0` be created.

Commit log files correspond to a snapshot. A commit log file should be named
`BASENAME-ssN-clM.piplog` where `M` is the commit log file number. The first
log file should have number `1` and subsequent files should be numbered one
greater than the largest number previously in use.

For now, partition data files are restricted to names matching one of the 
following regular expressions:

    ([0-9a-zA-Z\-_]+)-ss(0|[1-9][0-9]*).pip
    ([0-9a-zA-Z\-_]+)-ss(0|[1-9][0-9]*)-cl(0|[1-9][0-9]*).piplog
