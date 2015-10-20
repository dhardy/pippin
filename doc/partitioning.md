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
