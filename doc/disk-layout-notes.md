Data source
========

Single stream
-----------------

For: one snapshot (demo, data-recovery)

Interface: file name or stream.


Snapshot + commits
----------------------------

For: one snapshot + commits (demo, data-recovery)

Interface: all file names, base file name or snapshot file name (auto-detect
commit logs).


One partition
-----------------

For: single-partition repo or one partition of a repo, full commit support.

Interface: base file name or snapshot file name (auto-detect snapshots and
commit logs).


Whole repo
----------------

For: a whole repository or some sub-path of one.

Interface: specify directory, auto-detection of partitioning. Option to load
all partitions, load on demand or load only specific partitions.
