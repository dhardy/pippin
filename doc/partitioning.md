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
