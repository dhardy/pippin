- Pippin developer blog
- 22nd July 2017
- Author: Diggory Hardy

------

Hello and goodbye to partitioning
==============

First up, hello reader! So, what is this blog? Why hello and goodbye in the same title?

Pippin is undergoing some change right now. With that change comes a lot of thoughts
and documentation updates; rationales, removal of no-longer-applicable documentation
and code, and (eventually) development of some new features. But it seems a shame to
develop a rationale for a change, apply it, and delete all no-longer-applicable documentation
without any more comment than a few git commit messages and some short-lived
documentation notes. This isn't the first time I've seen the need for some significant
changes to Pippin, and I doubt it'll be the last. There are many other topics which might
get touched on some day, from plans for new features, to where I envision Pippin going,
to why it got started in the first place. And, maybe someday, Pippin will have actual users
to talk about their requirements and success stories.

To be clear, there *will* always be deletion of outdated documentation. At times I considered
moving such documentation to an archive/graveyard, which might be feasible when deleting
entire articles, having snippets cut from other documents and dumped in a graveyard still
make any sense implies considerable extra maintenance work, just disposing of outdated
documentation. Besides, git tracks history, so as long as the repository's history is still
available, that documentation can still be found if necessary.

But for now, lets get into the current changes and why they're happening.


Goodbye to partitioning
------------------------

From the start, Pippin was intended to be scalable to at least hundreds of megabytes and
millions of data items, if not much more. Although that scale is quite achievable with all data
loaded into memory on modern systems, it comes with significant RAM usage and start-up
times, which I wanted to keep low. My original solution to that was partitions: *partition*
data into multiple sub-sets, in a correlated fashion, such that only data currently of
interest need be loaded. It seemed like a reasonable match for my initial use-case:
email storage, including archives of old mails covering many years which would presumably
rarely be accessed. But, quite a *lot* of complications turned up while trying to implement
this feature...

### Partitioning: summary

Build multiple small *partitions*, each in effect a self-contained repository,
and assign each element to a partition, preferably in a correlated way such
that many operations on a subset of the whole data only touch a subset of
partitions.

### Decision to remove

After much thought on the issue, it has been decided to remove this feature,
and approach scalability by enabling operations without reading whole files first.

There are several problems with partitions:

*   Each partition has separate history, so commits do not span partitions and
    multi-partition transactions are impossible.
*   Creating a consistent view of data across multiple partitions, a "state"
    built from multiple per-partition states, is difficult:
    
    *   it must select at most one state per partition, but allow none, since
        there is little point to partitions if all must be loaded into memory
        anyway
    *   there is no good way to handle operations on unavailable partitions:
        loading-on-demand requires back-references to the partitions themselves,
        not just states, would cause huge delays accessing data, and means the
        user may have to handle a merge *during any get/set request*; failing
        the operation just forces the user to address this problem
*   Determining how to assign elements to partitions in a semi-automated way,
    allowing correlation without putting too much burden on the user, is
    complicated.
*   Discovering partitioning on start-up while keeping each partition's data
    files independent is tricky.
*   Many usage scenarios could require accessing some elements from each
    partition, negating any advantages to start-up speed or RAM usage, and
    performing very poorly if some partitions must be unloaded to free up
    enough RAM to use others.

But perhaps most critical is the problem with identifiers. There are two options:

*   Identify partition within the element identifier (e.g. as a prefix). If
    elements can ever move to another partition, either the identifier must
    change (causes problems for user) or it no longer corresponds to the current
    partition (see below). To avoid this, fixed partitions and limitations on
    element mutability are required (at which point the user has little reason
    not to use multiple indepedent repositories).
*   No embedded partition identifier. In this case, finding an element from an
    identifier requires checking all partitions, which makes multiple partitions
    less performant than a single one. Finding new unique identifiers is also
    hard.
*   A compromise: embedded partition identifiers which may be out-of-date.
    To make finding elements any better than above, partitions must now track
    all moved elements indefinetly (or at least until garbage collection), and
    repartitioning does not (in the short term) reduce the number of identifiers
    a partition needs to track.

### The plan

The `Repository` data structure, representing a collection of partitions, will
be removed, along with various supporting structs and traits. The existing
`Partition` struct will be renamed `Repository`, along with several supporting
types being renamed. [This is partially complete as of time of writing.]

It is likely that some breaking changes will still be needed to this new `Repository`
struct to support scalable usage (i.e. read and write operations without first reading all
current repository data), but I *think* it won't be necessary to completely revise
this interface to repository data, or use a multi-level API like the old `Partition` and
`Repository`.

With these changes done (and a little tidy-up), it seems reasonable to bump the
version number and call the result 0.2.0 alpha. Not a lot has changed since
0.1.0 feature-wise, but the removal of partitioning and resulting change in API is 
quite significant. At this point I'd be very happy for interested users to give Pippin
a try; some API breakage in the future is inevitable and entries can currently only be
quieried by an integer key, but I'm fairly confident at least that Pippin shouldn't lose
data due to its *very* conservative nature (currently only writing to new files).

As to scalability without partitions, more on that another time, but I intend to take
an approach more in common with other DBs (allocate space within a large file via
pages, construct look-up tables within the files themselves (likely using B-Trees),
while keeping Pippin's commit-oriented history and never-overwrite-historical-data
policy).
