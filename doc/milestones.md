Milestones for sync-sets development
========================


Store and load without history
----------------------------------------

Data files for snapshots. Library allows reading into memory, mutating, writing
out again.

*   necessary parts of file format done
*   code to load from and save to a snapshot file
*   code to access elements (identity, data) when in memory (read and modify or
    replace)

It also requires some basic setup:

*   some code conventions
*   rudimentary test framework

Done.


Simple store with history
--------------------------------

Limits: single file (no partitioning), no synchronisation support. Sets store
simple items, revisions store only the new version (no patches).

*   file format v1
*   simple data items (just plain data)
*   identifier for data items
*   identifiers for commits in change log

Tests:

1.  list contents of a given repository and output some items
2.  appending new items to history and verification of resulting file
3.  creation of an empty repository

Done apart from (1) tests and (2) log files should be extended.


Basic checksum and history operations
---------------------------------------------------

*   corruption detection
*   cloning repositories
*   reverting to older time points


Sync support
-----------------

Be able to connect to some remote repository via some mechanism and synchronise,
where there are no conflicting changes to individual items.

*   communication with external repositories (direct or via protocol)


Merge support
-------------------

Develop "patch" support for items and automatic merging under some situations.
Possibly the merging should work via a domain-specific plug-in.

*   details for data items
*   (possibly) a plug-in system for item validation, patches and merging


Time-based partitioning support
-------------------------------------------

Support snapshots and multiple files. Create new partitions given some criteria
(file size, age).

*   multiple file support
*   partition creation criteria
*   snapshots


Full partitioning support
---------------------------------

Support dividing a set over multiple files.

*   "paths" or some such to aid manual partitioning
*   loading and unloading of partitions
*   search across all partitions
*   partition-oriented API


Uncategorised items
--------------------------

*   Compression support
*   Encryption support
*   Checksums and repair functionality
