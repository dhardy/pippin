Partitioning Alternatives
========

Assumptions:

- large repo
- lots of history
- useful segregation of data possible (e.g. age)
- want to read and write only a small portion, quickly with little memory

Requirements:

- can find elements matching certain criteria without reading all data
- can write new data without reading much old data
- can store history in a small number of files


Existing partitioning
-------

Partitioning *does* solve the above, but not that well.

Good:

*   easy backups (snapshots don't change, logs only grow)
*   easy for user to delete history

Poor:

*   many files
*   all data in a partition must be read to read or write any
*   may have to read multiple data versions to find latest
*   some usage scenarios could easily require all partitions be read to access
    a small subset of data


Variations
----------

### Single partition

Use existing snapshot & log files, but only for a single partition.
Fewer files, but must read *all* data. Might work better in combination with something else,
but is in any case good enough for small repositories.


### COW data + flat file

When a piece of data changes, just consider the new version to be a new piece of data. Store the
versioned data in a flat file via some allocation scheme (e.g. like SQLite).

Advantages:

*   only one file

Problems:

*   can only backup whole file, which is likely different from but similar to the last backup
*   deleting history must be done carefully by the library
*   all modifications touch important data file and risk corruption
*   merging historical data from two versions (e.g. backup and current) is hard


### Log files, read latest first

By reconstructing recent history backwards, deleted/replaced data can be ignored. The cost of
doing this is less verification and not having old versions available in memory, but much of the
time this may not matter.

### Data pointers

Don't store large data in-place; store it elsewhere (e.g. a growable blob file). Keep small data
in-place. This may improve read performance. Blob files may be hard to backup.


### Versioned B-trees

Store data via a B-tree mapping from keys to either in-place data or data pointers. Use pointers
to link the next block in a B-tree. But:

*   allow commits in logs to override pointers with a new data, with a version
*   pointer data in snapshot all has version 0

On startup, build:

*   a hash-map of commits by commit-hash
*   a map from pointers to (a map/vec from versions to data)

Advantages:

*   allows opening a large DB without parsing actual data
*   should be scalable to multiple key tables pointing to same data (property tables)
*   works with grow-only snapshot+log+blob files

Disadvantages:

*   initialisation of a large DB still slow

Questions:

*   Is there still any advantage to partitioning? Many searches would likely need to operate on all
    partitions anyway.
