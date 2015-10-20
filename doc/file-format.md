File format
========

TBD means To-Be-Defined. "Later" indicates sections which are not included in
the current version but are planned (informally) for later versions. All parts
of the format may change but this requires updating the header. The format is
not currently considered stable.

Chunks are aligned on 16-byte boundaries. Note: this may waste a fair bit of
space.

Types: u8 refers to an unsigned eight-bit number (also a byte), u64 a 64-bit
number, i8 a signed byte etc. (these are Rust types). These are written in
binary big-endian format. (There is no strong reason for chosing big-endian.)

Text must be ASCII or UTF-8. User-defined data is binary (u8 sequence).

Checksums are in whichever format is mentioned in the header. All options start
`SUM` to be self-documenting. Currently available options: `SUM SHA-2 256 `.
They are encoded as [bytes or single number?] TBD. Lots of checksums are
written; this may waste space.

### Identifiers

Most identifiers will be ASCII and right-padded to 8 or 16 bytes with space
(0x20) bytes, or they will be binary.

File format: 16 bytes: `PIPPINxxyyyymmdd` (e.g. `PIPPINSS20150916`). `xx` is
repaced with two letters (e.g. `SS` for snapshot files and `CL` for commit
logs), and `yyyymmdd` with the date of format specification. It is expected
that many versions get created but that few survive to the release stage.



Snapshot files
========

NOTE: the `Bbbb` variant is not currently implemented and may be excluded.

Header
----------

*   `PIPPINSS20150929` (PIPPIN SnapShot, date of last format change)
*   16 bytes UTF-8 for name of repository; this string is identical for each
    partition and right-padded with zero (0x00) to make 16 bytes
*   header content
*   checksum format starting `HSUM` (e.g. `HSUM SHA-2 256`)
*   checksum of header contents (as a sequence of bytes)

Where it says "header content" above, the following is allowed:

*   A 16-byte "line" whose first byte is `H` (0x48); typically the next few
    bytes will indicate the purpose of the line as in `HSUM`.
*   A variable-length section starting `Qx` where x is a base-36 number (1-9 or
    A-Z); 'Q' for 'quad word'.
*   A variable-length section starting `Bbbb` where 'bbb' is a big-endian
    24-bit number and signifies the number of bytes in the section (including
    the `Bbbb` part). The length must be a multiple of 16.

NOTE: the `Bbbb` variant is not currently included.

These allow extensible header content. Extensions should use the first of these
variants which is suited to their application in order to keep the header as
readable as reasonably possible in a hex-editor. Typically the first few bytes
following the `H`, `Qx` or `Bbbb` will identify the purpose of the block as in
`HSUM` for the checksum format specification.

The next section deals with recognising what these blocks contain, starting
from the byte following `H`, `Qx` or `Bbbb`. Typically blocks are right-padded
with zero bytes when the content is shorter than the block length.

### Header blocks

Remark blocks start `R` and should be UTF-8 text right-padded with zeros.

User fields of the header start `U` and are passed through to the program
using the library as byte sequences (`Vec<u8>` in Rust terminology).

File extensions start with any other capital letter (`A-Z`); ones starting `O`
are considered optional (i.e. interpreters not understanding them should still
be able to read the file) while others are considered important (interpreters
not understanding them are likely to fail).

Blocks starting with anything other than a capital letter are ignored if not
recognised.

#### Checksum format

Block starts `SUM`.
It is used to specify the checksum algorithm used for state checksums.
(Note that the checksum used for verifying the file's header contents, snapshot
and commit contents as written in the file is fixed to SHA-256.) This section
is special in that it must be the last section of the header; i.e. the next n
bytes (32 in the case of SHA-256) are the checksum and terminate the header.

Currently only `SUM SHA-2 256` is supported.

#### Other

TBD: information on partition, parent, etc.


Snapshot
------------

Data is written as follows:

*   `SNAPSHOT` (section identifier)
*   (??) the date of creation of the snapshot as YYYYMMDD
*   TBD: state/commit identifier and time stamp
*   `ELEMENTS` (section identifier)
*   number of elements as a u64 (binary, TBD endianness)

Per-element data:

*   `ELEMENT` to mark section (pad to 8 bytes with zero)
*   element identifier (u64)
*   `BYTES` (padded to 8) to mark data section and format (byte stream)
*   length of byte stream (u64)
*   data (byte stream), padded to the next 16-byte boundary
*   checksum (TBD: could remove)

Finally:

*   `STATESUM` (section identifier)
*   number of elements as u64 (again, mostly for alignment)
*   state checksum (doubles as an identifier)
*   checksum of data as written in file


Log files
======

Header
---------

The header has the same format as snapshot files except that the first 16 bytes
are replaced with `PIPPINCL20150924`.

Header content (`H...`, `Q...`,  `B...` sections) may differ.


Commit log
----------------

Section identifier: `COMMIT LOG      `.

List of commits, weakly ordered (parent must come before child, but siblings
may be listed in any order).

### Commits

NOTE: merge commits will look a little different!

Each commit should start:

*   with an idenfitier: `COMMIT`
*   a timestamp TBD
*   parent commit id / state sum
*   length of commit data OR number to elements changed (?)
*   PER ELEMENT DATA
*   a state checksum
*   a checksum of the commit data (from start of the commit to just before
    this checksum itself)

### Per element data

Where "PER ELEMENT DATA" is written above, a sequence of element-specific
sections appears. The syntax for each element is:

*   section identifier: `ELT ` followed by one of `DEL` (delete), `INS` (insert
    with new element id), `REPL` (replace an existing element with new data) or
    TODO `PATC` (patch an existing element)
*   element identifier (partition specific, u64)

Contents now depend on the previous identifier:

*   `DEL`: no extra content
*   `INS`: identifier `ELT DATA`, data length (u64), data (padded to 16-byte
    boundary with \\x00), data checksum (32 bytes, used to calculate the state
    sum)
*   `REPL`: contents is identical to `INS`, but `INS` is only allowed when the
    element identifier was free while `REPL` is only allowed when the
    identifier pointed to an element in the previous state.


Repository disk layout
==============

This isn't strictly a file format.

A repository is saved as a collection of snapshot files and log files.

Filenames are of the form `BASENAME-NUMBER.EXT` where `BASENAME` may include
`/` (interpreted as folder path separators on the file system). Other
characters allowed in the `BASENAME` are TBD.
Should have some base name, some version number and some extension.
`NUMBER` is a non-negative decimal number which is incremented by one every
time a new snapshot is made, starting from 0 for an empty snapshot or 1 for a
non-empty initial snapshot.
`EXT` should be `pip` for snapshot files and `pipl` for commit logs.

### Partitioning
Different base names should be used for each partition; additionally folders
could be used; from the point of view of the library this is simply one base
name possibly containing `/` separators.

Base names may be specified by the program using the library (e.g. generated
from classifiers) or even by the end-user of the program.

TODO: try to work out a scheme whereby some information about partitions can
be recovered without reading all partition headers.

### Repartitioning
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

Simplest solution
----------------------

Partitions are given new names on the disk not correlating to partition path or
any other user-friendly naming method. Renaming paths thus does not move
partitions. All partitions are stored in the same directory. Partitions are
never removed, but left empty if no longer needed.

Allowing partition removal
------------------------------

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

Log files
----------

Log files *always* correspond to some snapshot; there may be multiple log files
corresponding to a snapshot. As such log files should use the snapshot file
name with a postfix number/letter and maybe a different extension.
