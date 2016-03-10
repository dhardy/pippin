<!-- This Source Code Form is subject to the terms of the Mozilla Public
   - License, v. 2.0. If a copy of the MPL was not distributed with this
   - file, You can obtain one at http://mozilla.org/MPL/2.0/. -->

File format
========

This document describes the content of files. The file [repo-files.md]()
describes how these files are stored on the disk.

### Versions

The following versions are specified:

*   2016 03 10 — new version for new checksums

Older versions are not supported since supporting old statesums would be hard
and there is no usage outside of test-cases.


Potential changes
---------------

These may or may not be acted upon.

*   identify snapshot file in change logs (cannot do vice-versa since snapshot
    is written first, except for guessing the file name, but we can already do that)
*   identify previous snapshot(s) and possibly change logs leading up to a new
    snapshot
*   list classifiers by name and maybe output restrictions
*   describe classifier restrictions for the current partition
*   describe all classifier restrictions / other known partitions


Terms
-------

TBD means To-Be-Defined. "Later" indicates sections which are not included in
the current version but are planned (informally) for later versions. All parts
of the format may change but this requires updating the header. The format is
not currently considered stable.

Chunks are aligned on 16-byte boundaries. Note: this may waste a fair bit of
space.

Types: u8 refers to an unsigned eight-bit number (a byte), u64 a 64-bit
number, i8 a signed byte etc. (these are Rust types). These are written in
binary big-endian format. (There is no strong reason for chosing big-endian.)

Text must be ASCII or UTF-8. User-defined data is binary (u8 sequence).

Checksums are in whichever format is mentioned in the header. All options start
`SUM` to be self-documenting. Currently available options: `SUM SHA-2 256 `.
They are encoded as unsigned bytes.
#0016 Lots of checksums are written; this may waste space.

### Identifiers

Most identifiers will be ASCII and right-padded to 8 or 16 bytes with space
(0x20) bytes, or they will be binary.

File format: 16 bytes: `PIPPINxxyyyymmdd` (e.g. `PIPPINSS20160310`). `xx` is
repaced with two letters (e.g. `SS` for snapshot files and `CL` for commit
logs), and `yyyymmdd` with the date of format specification. It is expected
that many versions get created but that few survive to the release stage.



Snapshot files
========

NOTE: the `Bbbb` variant is not currently implemented and may be excluded.

Header
----------

*   `PIPPINSS20160310` (PIPPIN SnapShot, date of last format change)
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
It is used to specify the checksum algorithm used for (a) calculating state
checksums and (b) verifying the file's header contents, snapshot
and commit contents. (Originally (b) was fixed since it was impractical to
change at run-time, but (a) is also impractical to change at run-time, hence
this currently indicates what the program is compiled to work with.)

This section is special in that it must be the last section of the header; i.e.
the next n bytes (16 in the case of BLAKE2 16) are the checksum and terminate
the header.

Originally supported: `SUM SHA-2 256`. Now, only `SUM BLAKE2 16` is supported.

#### Partition number

Each partition has a unique 40-bit number, called the partition number. It is
stored in the high 40 bits of a `u64` (where the low 24-bits are zero), and
called a "partition identifier".

This is stored in a header block starting `PARTID ` then continuing with a
`u64`.

#### Other

TBD: information on partition, parent, etc.


Snapshot
------------

Data is written as follows:

*   `SNAPSH` (section identifier), a byte (u8) indicating the number of
    parents, `U` (8 bytes total)
*   UNIX timestamp as an i64
*   `CNUM` (commint number) followed by a `u32` (four byte) number, which is
    the commit number (max parent number + 1; not guaranteed unique)
*   `XM`, two more bytes, a `u32` (four bytes unsigned) number; this is the
    "extra metadata" section, the two bytes may be zero-bytes (ignore data) or
    `TT` (UTF-8 text) or anything else (future extensions; for now
    implementations will probably ignore data), the four byte number is the
    data length (next bit)
*   Extra metadata: length is defined above; section is zero-padded to a
    16-byte boundary. Generally it is safe to ignore this data, but users may
    store extra things here (e.g. author and comment).
*   for each parent (see `SNAPSH` above), its state sum; length depends on
    checksum algorithm
*   TBD: state/commit identifier and time stamp
*   `ELEMENTS` (section identifier)
*   number of elements as a u64

Per-element data (in any order):

*   `ELEMENT` to mark section (pad to 8 bytes with zero)
*   element identifier (u64)
*   `BYTES` (padded to 8) to mark data section and format (byte stream)
*   length of byte stream (u64)
*   data (byte stream), padded to the next 16-byte boundary
*   checksum (TBD: could remove)

Memory of moved elements; this section is optional and jused to track elements
moved to other partitions. If no moves have been tracked it may safely be
omitted.

*   `ELTMOVES` to mark section
*   number of records (u64)
*   for each record,
    
    1.  the source identifier
    2.  the new identifier after the moveq

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
are replaced with `PIPPINCL20160310`.

Header content (`H...`, `Q...`,  `B...` sections) may differ.


Commit log
----------------

Section identifier: `COMMIT LOG      `.

List of commits, weakly ordered (parent must come before child, but siblings
may be listed in any order).

### Commits

NOTE: merge commits will look a little different!

Normal commits start with the identifier `COMMIT` (6 bytes).
Merge commits start with the identifier `MERGE`, followed by a `u8` (unsigned
byte) indicating the number of parents (must be at least two); again 6 bytes.

This is followed by:

*   `\x00U` (2 bytes: zero U), indicating that a UTC UNIX timestamp follows
*   an `i64` (eight byte signed) UNIX timestamp (the number of non-leap seconds
    since January 1, 1970 0:00:00 UTC) of the time the commit was made
*   `CNUM` (commit number) followed by a `u32` (four byte) number, which is
    the commit number (max parent number + 1; not guaranteed unique)
*   `XM`, two more bytes, a `u32` (four bytes unsigned) number; this is the
    "extra metadata" section, the two bytes may be zero-bytes (ignore data) or
    `TT` (UTF-8 text) or anything else (future extensions; for now
    implementations will probably ignore data), the four byte number is the
    data length (next bit)
*   Extra metadata: length is defined above; section is zero-padded to a
    16-byte boundary. Generally it is safe to ignore this data, but users may
    store extra things here (e.g. author and comment).
*   for each parent (one for `COMMIT`, two or more for `MERGE`; see above), its
    state sum; length depends on checksum algorithm
*   length of commit data OR number to elements changed (?)
*   PER ELEMENT DATA
*   a state checksum
*   a checksum of the commit data (from start of the commit to just before
    this checksum itself)

Note that there must be at least one parent to a commit, and the first parent
is the one to which this commit is the "diff" (can be patched onto to derive
the commit's state).

### Per element data

Where "PER ELEMENT DATA" is written above, a sequence of element-specific
sections appears. Elements may appear in any order. The syntax for each
element is:

*   section identifier: `ELT ` followed by one of
    
    *   `DEL` (delete)
    *   `INS` (insert with new element id)
    *   `REPL` (replace an existing element with new data)
    *   `MOVO` (moved out, that is `DEL` plus a new identifier)
    *   `MOV` (moved, that is a new identifier but no operation on stored elements)
    *   (TODO) `PATC` (patch an existing element)
*   element identifier (partition specific, u64)

Contents now depend on the previous identifier:

*   `DEL`: no extra content
*   `INS`: identifier `ELT DATA`, data length (u64), data (padded to 16-byte
    boundary with \\x00), data checksum (used to calculate the state sum)
*   `REPL`: contents is identical to `INS`, but `INS` is only allowed when the
    element identifier was free while `REPL` is only allowed when the
    identifier pointed to an element in the previous state.
*   `MOVO` or `MOV`: identifier `NEW ELT` (pad to 8 bytes), element identifier
    (u64)

