File format
========

TBD means To-Be-Defined. "Later" indicates sections which are not included in
the current version but are planned (informally) for later versions.

Chunks are aligned on 16-byte boundaries.

Text must be ASCII or UTF-8. User-defined data is binary.

Checksums are in whichever format is mentioned in the header. All options start
`SUM` to be self-documenting. Currently available options: `SUM SHA-2 256 `.
They are encoded as [bytes or single number?] TBD.


Snapshot files
========

NOTE: the `Bbbb` variant is not currently implemented and may be excluded.

Header
----------

*   `PIPPINSS20150924` (PIPPIN SnapShot, date of last format change)
*   16 bytes UTF-8 for name of repository; this string is identical for each
    partition and right-padded with zero (0x00) to make 16 bytes
*   header content
*   checksum format starting `HSUM` (e.g. `HSUM SHA-2 256  `)
*   checksum of header contents

Where it says "header content" above, the following is allowed:

*   A 16-byte "line" whose first byte is `H` (0x48); typically the next few
    bytes will indicate the purpose of the line as in `HSUM`.
*   A variable-length section starting `Qx` where x is a base-36 number (1-9 or
    A-Z); 'Q' for 'quad word'.
*   A variable-length section starting `Bbbb` where 'bbb' is a big-endian
    24-bit number and signifies the number of bytes in the section (including
    the `Bbbb` part). The length must be a multiple of 4 and is preferred to
    be a multiple of 16.

These allow extensible header content. Extensions should use the first of these
variants which is suited to their application in order to keep the header as
readable as reasonably possible in a hex-editor. Typically the first few bytes
following the `H`, `Qx` or `Bbbb` will identify the purpose of the block as in
`HSUM` for the checksum format specification. If the program reading the header
does not recognise the block it may be ignored.

### Header blocks

A line starting `HSUM` is used to specify the checksum format used throughout
the file. This section is special in that it must be the last section of the
header; i.e. the next n bytes (32 in the case of SHA-256) are the checksum and
terminate the header. TBD: binary format of checksum (big- or little-endian
bytes?).

TBD: information on partition, parent, etc.


Snapshot
------------

Section identifier: `SNAPSHOT` followed by the date of creation as YYYYMMDD.
(TBD: replace date with something else? It's not essential.)

Commit identifier (TBD)

All data from partition (TBD)


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

*   with an idenfitier: `COMMIT` (?)
*   parent commit
*   a timestamp TBD
*   length of commit data OR number to elements changed (?)
*   PER ELEMENT DATA
*   a state checksum
*   a checksum of the commit data (from start of the commit to just before
    this checksum itself)

### Per element data

Where "PER ELEMENT DATA" is written above, a sequence of element-specific
sections appears. The syntax for each element is:

*   element identifier (partition specific, u64)

Each commit contains:

*   16-byte commit identifier
*   identifier of each parent commit
*   date & time of commit
*   list of items changed; for each, one of: a marker such as MOVED or DELETED,
    the full data of the item, or a patch
*   checksum of commit contents

TBD: how is the number of parents specified? Should there be something to make
it clear when scanning the file where the commit starts?
