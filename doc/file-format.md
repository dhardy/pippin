File format
========

TBD means To-Be-Defined. "Later" indicates sections which are not included in
the current version but are planned (informally) for later versions.

Chunks are aligned on 16-byte boundaries.

Text must be ASCII or UTF-8. User-defined data is binary.

Checksums are in whichever format is mentioned in the header. All options start
`SUM` to be self-documenting. Currently available options: `SUM SHA-2 256 `.
They are encoded as [bytes or single number?] TBD.


Header
----------

*   `PIPPIN 20150916 ` (date of format creation)
*   16 bytes for name of repository (text); this string is identical for each
    partition and right-padded with spaces (0x20) to make 16 bytes
*   (later) information on parent(s)
*   (later) information on partition (TBD)
*   checksum format (e.g. `SUM SHA-2 256 `)
*   checksum of header contents


Snapshot
------------

Section identifier: `SNAPSHOT` followed by the date of creation as YYYYMMDD.
(TBD: replace date with something else? It's not essential.)

Commit identifier (TBD)

All data from partition (TBD)


Commit log
----------------

Section identifier: `COMMIT LOG      `.

List of commits, weakly ordered (parent must come before child, but siblings
may be listed in any order).

Each commit contains:

*   16-byte commit identifier
*   identifier of each parent commit
*   date & time of commit
*   list of items changed; for each, one of: a marker such as MOVED or DELETED,
    the full data of the item, or a patch
*   checksum of commit contents

TBD: how is the number of parents specified? Should there be something to make
it clear when scanning the file where the commit starts?
