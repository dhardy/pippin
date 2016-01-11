Partitioning
=======

Starting point
---------

A *partition* is defined from the API point of view: a subset of elements which
are loaded and unloaded simultaneously. The partition-file relationship is
1-many: any particular state of the partition's history is fully encapsulated
in a single file, but the full history may span multiple files. Any file will
contain the entire state of some partition at some point in time, and likely
multiple points in time via changesets and/or multiple snapshots.

*Partitioning* of the repository into partitions is neither user-defined nor
fixed. User-defined classifiers are used when designing new partitions in a
user-defined order. A partition will be defined according to some range of
allowable values from one or more classifiers. If insufficient classifiers are
present to give desirable granularity of partitions, the library can do no more
than warn of this.

### When to partition

The criteria for creating further partitions should be set by the user. Some
use-cases might be happy with large data-sets which are slow to load and
repartition but are still fast to read and fairly fast to write to; other cases
might be happier with many small partitions which can be loaded or
repartitioned nearly instantaneously.


Meta-data
---------------

We can store some data about partitioning in the partitions themselves. For
all known partitions:

*   classification data for the partition
*   classification enumeration and/or partition number for element identifiers
*   base-name of partition files on disk
*   status (whether closed or not)

This information may be out-of-date for some partitions, but provided we never
*change* a partition but only sub-divide into *new* partitions, it's quite easy
to spot what is out of date: either there will be no files on disk, or when
loaded, the files for that partition will say that it has been closed.

In memory, we use the most up-to-date information available (with some simple
mechanism for solving conflicts about information on unloaded partitions
received from loading other partitions, e.g. using a version number attached to
the partition info). When creating new partitions, we write all we know.


Classification of elements
------------------

### Use-cases

Partitioning of emails by (a) date-of-sending, (b) organisation of sender
(match email to glob or regex, match to a list, or leave in the "default"
category), (c) by size.

### Possible solutions

#### Numeric property functions

Classifiers are user-defined functions mapping elements to a number, which is
either a user defined enumeration or a user-defined range of integers (TBD:
32-bit? signed?).

Problem: doesn't work for *string* properties (like an email's sender).

#### Polymorphic property functions

Classifiers are user-defined functions mapping elements to either a number or a
string (as an enum), which can then be matched against a classifier externally.

Advantages:

*   actual classification can be done externally from the element
*   a custom classifier can be implemented for searches using any desired
    operations on these properties

Disadvanatages:

*   properties must be fixed
*   for any property not either a number or a slice of existing element data
    this is inefficient

#### Internal classification

Classifications are rule-sets which either match or don't match some given
property, or logical combinations of such matchers. Classifiers are sets of
prioritised (or exclusive) classifications.

Advantages:

*   properties can be provided on-demand, but must still be fixed

Disadvantages:

*   matching logic must be internal to element type
*   logic available must be fixed?

#### External classifiers

We use compile-time polymorphism (otherwise known as templating or "generics")
to support custom *element* types. This means that it is trivial to extract the
underlying element type for use with a custom *classifier*.

Our classifier can thus be an external entity which can access details of
elements in whichever way the user of the library sees fit and returns a
classification into categories, whether the properties involved are integers,
strings or whatever else.

To allow the Pippin library to determine when to sub-divide a category but
leave the user the rest of the control, all we need is that the classifier
enumerates categories and, on request, will subdivide one category into two or
more sub-categories (or fail, if no more classification is possible).

This leaves two problems: (1) the user must write quite a bit of code to deal
with classification, and (2) the user-defined categories must be preserved. We
cannot completely solve (1) while leaving the user in control, but can provide
templates, and besides, classification is of little importance for small
databases. For (2), one solution would be to make the user provide a "matcher"
object tied to each output enumeration, in order of matching priority, with
serialisation and deserialisation support. Unfortunately matching against a
long list of classifiers until a match is found is not very efficient. Another
option might be to use a single "classifier" object and serialise the whole
thing in one go.

The next problem is where to put the "classifier" once it's been serialised. It
might be better to leave this up to the user? Except that (a) it should be
synchronised simultaneously with other repo data and (b) it wouldn't be good to
reclassify only to have details of the new classifier forgotten. Maybe a better
approach would be to let the user save, in each partition, details of that
partition's classification (in whatever form it wishes)?

--------------

Each function must have a name (TBD: allowable identifiers).

The names of classifier functions and their domains (enumeration or integer
range) are recorded in each snapshot and may not change. The functions
themselves are provided by the user and *should* not change; if they do, the
library will provide options for dealing with this (search by iteration over
all elements of a partitioning, reclassification of all elements in a
partition), but searching/filtering by classifiers may not return the correct
elements until reclassification is complete.

Classifier functions may not be removed (as a work-around, they can be changed
to output only a single value and values reclassified). New classifier
functions may be added with lowest priority but will only be recorded in new
snapshots.

Classifier functions must not change their domains (relative to what was
recorded as being in use by previous snapshots).
Classifier functions previously available (when previous snapshots were made)
must continue to be available.

Priority of the classifier functions is user defined. This is recorded in
snapshots but may be changed; however partitioning will not be changed until
either the user or the library decides to repartition.
In the mean time, existing snapshots may or may not be updated
with the new priorities while new snapshots will record the new priorities but
continue to represent the old partition.

### Partitioning from classifier values

Desirable: partitioning happens at "sensible" boundaries, e.g. by year, then
perhaps by month or quarter, not just some date or time during the year.
Further, it should be possible to label partitions based on these values (e.g.
year or month).

This can be done by using multiple prioritised classifiers (e.g. "year",
"quarter", "month", "week", "day-of-month"). This is however more to set up and
more classifications to remember per-element than a single date/time-stamp.

Alternatively, a single classifier might be used along with rules about where
best to partition ranges. Perhaps most simply the function would take a range
and return two or a small number of sub-ranges.

The system might or might not also want to predict which new partitions might
be needed in the future. For example, if elements are partitioned by date added
with new elements continually being added, and recent elements have been
partitioned by year then quarter, it might make sense to create new partitions
by quarter proactively instead simply of when a partition gets too big.


Partition identification
--------------------

### Labels on the disk

TBD: how to identify a partition in memory.

Standard file extensions are `.pip` for snapshot files (it's short, peppy, and
self-contained), and `.piplog` for commit log files (which must correspond to a
snapshot file).

Given some `BASEPATH` (first part of the file name, potentially prefixed by a
path), snapshot files are named `BASEPATH-ssN.pip` where `N` is the number of a
snapshot. The first non-empty snapshot is usually numbered `1`; subsequent
snapshots should be numbered one more than the largest number in use (not a
hard requirement). The convention for new partitions is that an empty snapshot
with number `0` be created.

Commit log files correspond to a snapshot. A commit log file should be named
`BASEPATH-ssN-clM.piplog` where `M` is the commit log file number. The first
log file should have number `1` and subsequent files should be numbered one
greater than the largest number previously in use.

This `BASEPATH` is an arbitrary Unicode string except that (a) it may contain
path separators (`/` on all operating systems), and (b) the part after the last
separator must be a valid file name stem and the rest must either be empty or
a valid (relative or absolute) path. To aid users, names may be suggested by
the program using the library.

For now, partition data files are restricted to names matching one of the 
following regular expressions:

    ([0-9a-zA-Z\-_]+)-ss([1-9][0-9]*).pip
    ([0-9a-zA-Z\-_]+)-ss([1-9][0-9]*)-cl([1-9][0-9]*).piplog

### Unique partition numbers.

How can we ensure that each partition gets a unique number?
These are needed to ensure elements get unique numbers.

Note: an enumeration is needed for classification. It would probably be a good
idea either to use that enumeration here or to use these numbers for the
enumeration.

Bifurcation
------------

Start at some number, e.g. 2^31 if we have a 32-bit unsigned int.

Every time a partition is split, create two new identifiers by setting the
most significant unset bit to 1 and in one case setting the next bit to 0.
Remove the previous number from use so that it is not used to reproduce these
numbers again.

Disadvantage: only 31 levels of splitting possible with 32 bits; less if
splitting into more than two new partitions at a time. Old numbers cannot be
reused.

Linear splitting
------------

Adaptation of above, where each partition *remembers* the range it has
available, and divides this up among child partitions when partitioning. This
is considerably more efficient when splitting to more than two child
partitions.

It does have one drawback: if elements are not relabelled, then checking
uniqueness of new elements with the same partition number is not easy, so
probably additionally partitions must *remember* which numbers within their
range they are not able to use.

Redistribution
-------------

This could be added to either of the above, presumably when new partitions are
needed.

The idea is simple: seek out partitions with more numbers available than they
need, steal some (updating those partitions with their new number/range), and
assign these to new partitions.

There is some risk: if the program crashes at the wrong time, it might either
lose some numbers or double-assign them. Further, another process using the
library could *theoretically* divide the partition from which these numbers are
stolen at the same time, causing more problems.


Problems
=======

Identifiers
----------

Details remain sketchy. See above.


Repartioning
---------------

Problem: a repository is partitioned. Each partition is independent and may
not have knowledge of other partitions.

Partitioning is not fixed, but must always be disjoint (no overlaps) and
covering (no gaps).

The user wishes to be able to determine where some element should be placed
or can be found without analysing all partitions.


Repartitioning (also moving elements)
----------

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


Changing classifiers
------------

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


Discovering partition files
---------------------

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

### Naming snapshot files

In light of this, it might be best if snapshot files have the form
`BASE-NUM-SUM.pip` where `BASE` is the basename (common within the partition),
`NUM` is a sequential number, and `SUM` is part of the snapshot state-sum. This
should make sure states in a "branched" repository have different names.

### Commit logs

These should have the same `BASE-NUM-SUM` part as the corresponding snapshot
file. They should then have some unique identifier.

Option 1: a number.

Option 2: the state-sum of the latest state. This requires writing from scratch
or renaming on every new commit, which isn't ideal.


Writing new commits
------------------

When new commits are written:

*   existing data must not be corrupted, which means a log file must not be
    modified unless a replicate exists
*   log files should not be written from scratch unnecessarily

Possible methods:

1.  Remember all commits since the last snapshot. On write-to-disk, make a new
    commit log containing all these. Delete redundant logs another time.
2.  Variant of 1: when loading, write a new commit log. When writing new
    commits to disk write to this log (so there is a chance of corrupting
    commits from the same session).
3.  Variant of 1: on first write-to-disk, make a new commit log. On the second
    write, create another log file. Following this, alternate between the two
    logs (also writing commits not yet in that log file).

These options *only* write to log files created in the current session. Why?
Because this way (providing other clients do the same) there is no chance of
changes conflicting with changes from another client. But maybe this isn't
necessary.

**Atomic writes:** format a commit in memory, then append atomically. I think
on some systems such writes are guaranteed not to conflict with writes from
other processes (but may interleave). If divergent states are created by
commits in a single log, this doesn't matter (merge will happen on reading).
This may or may not be reliable; [according to this report there are length
restrictions which make this unsuitable](http://stackoverflow.com/questions/1154446/is-file-append-atomic-in-unix),
however [these limits may be attributable to other language's buffering
features](https://github.com/rust-lang/rfcs/pull/1252#issuecomment-153290345).

**Verifying writes:** don't assume that simultaneous writes can't clash, but
verify the commits just written by re-reading the relevant part of the file.

**Partial logs:** as above, use new files for each session, but don't re-write
old content (at least, not always). Advantage: less writing. Disadvantage: a
major prolification of log files.

Using the above, we have some more possibilities:

4.  Select the lowest-numbered existing log which does not contain any commits
    not also written elsewhere; if there is none then create a new log. Open in
    apend mode for atomic writes. Write all existing commits not already in the
    log, then any new commits. Write each commit as a single write so that it
    is appended atomically.
5.  Variant of (4), but every so often (number of commits? timeout?) close the
    file, re-read it to verify its contents, and select a new log file
    according to the same algorithm.


Commit size
-----------

The above write algorithms do not place any hard limits on commit size. Still,
reducing the number of commits would be good for performance and avoiding
*large* commits (for some definition of *large*) *may* be a good idea.


Commit log clean-up
----------------------

*If* many log files get created (see above write policies), a deletion policy
is "needed". Assume each log file has a number.

1.  On load, read *all* logs and delete any which are entirely redundant with
    some higher-numbered log-file. Issue: there is no guarantee that a process
    is not still writing to one of these files.
2.  As above, but set some "stale" age. Only delete "stale" files. Don't ever
    write new commits to a "stale" file. Issue: there is no upper-bound on how
    long a commit may take to write. Issue: time stamps may not be reliable
    (maybe "creation time" metadata is okay?).
3.  As (1) but write the owning process PID to the log. Not okay across
    network file systems; not portable?
4.  Some time after a new snapshot is created, go back and write a unified log
    and delete old log files. Same issues as (2), plus cannot limit log files
    except by creating new snapshots (which also have their drawbacks).

No option is perfect. Perhaps (2) with a generous bound on write time assumed
(e.g. only delete files 24 hours after they become stale) is acceptable.
