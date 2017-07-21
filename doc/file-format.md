<!-- This Source Code Form is subject to the terms of the Mozilla Public
   - License, v. 2.0. If a copy of the MPL was not distributed with this
   - file, You can obtain one at http://mozilla.org/MPL/2.0/. -->


File format
========

This document describes the content of files. The file [repo-files.md]()
describes how these files are stored on the disk.


Versions
-------------

The below describes how to read and write the latest version.
It also describes how to read features from older versions (denoted legacy).

The following versions are specified:

*   2016 08 15 — allow non-breaking extensions to commit-meta
*   2016 05 16  — support Bbbb header sections
*   2016 03 10 — new version for new checksums

No older versions are supported since the checksum algorithm changed and
supporting older algorithms would add complexity without being very useful.


Potential changes
---------------

These may or may not be acted upon.

*   identify snapshot file in change logs (cannot do vice-versa since snapshot
    is written first, except for guessing the file name, but we can already do that)
*   identify previous snapshot(s) and possibly change logs leading up to a new
    snapshot
*   #0016 Lots of checksums are written; this may waste space.


Terms
-------

**Essential** means something that interpreters must understand to use the file
correctly; if they do not understand an essential thing they should either stop
or proceed with caution (perhaps extra verbosity) and not normally save new
versions or do anything important with the data. The opposite are
**inessential** features which can safely be ignored if not understood.

**Chunks** are aligned on 16-byte boundaries. Note: this may waste a fair bit
of space; reducing to 8-byte boundaries may be sensible.

Types: **u8** refers to an unsigned eight-bit number (a byte), **u64** a 64-bit
number, **i8** a signed byte etc. (these are Rust types). These are written in
binary big-endian format. (There is no strong reason for chosing big-endian.)

**Text** must be ASCII or UTF-8. **User-defined data** is binary (u8 sequence).


Header section
=========

This is common to both snapshots and commit logs, except for the first line.

Header
----------

The header starts with one of:

*   `PIPPINSS20160815`
*   `PIPPINCL20160815`

this encodes `PIPPIN`, the type of file (SnapShot or Commit Log) and the
file format version (in the form of the date on which it was stabilised). This
is followed by:

*   16 bytes UTF-8 for name of repository; this string is identical for each
    file in the repository and right-padded with zero (0x00) to make 16 bytes
*   header content (see below)
*   checksum format starting `HSUM` (e.g. `HSUM SHA-2 256`)
*   checksum of header contents (as a sequence of bytes)

### Header content

Where it says "header content" above, the following is allowed:

*   A 16-byte "line" whose first byte is `H` (0x48); typically the next few
    bytes will indicate the purpose of the line as in `HSUM`.
*   A variable-length section starting `Qx` where x is a base-36 number (1-9 or
    A-Z); 'Q' for 'quad word'. The section (including `Qx`) has length `16*x`.
*   A variable-length section starting `Bbbb` where 'bbb' is a big-endian
    24-bit number and signifies the number of bytes in the section (including
    the `Bbbb` part). The length of the section (including `Bbbb`) is this
    24-bit number rounded up to the next 16-byte boundary.

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

Blocks starting with any other capital letter (`A-Z`except `R` and `U`) are
considered essential (see terminology above). Blocks starting with any lower-
case letter (`a-z`) are considered inessential and may be ignored. Blocks
starting with anything else (not a letter) are not allowed.

#### Checksum format

Format: `SUM BLAKE2 16` (zero-padded).

This is used to specify the checksum algorithm used for (a) calculating state
checksums and (b) verifying the file's header contents, snapshot
and commit contents. (Originally (b) was fixed since it was impractical to
change at run-time, but (a) is also impractical to change at run-time, hence
this currently indicates what the program is compiled to work with.)

This section is special in that it must be the last section of the header; i.e.
the next n bytes (16 in the case of BLAKE2 16) are the checksum and terminate
the header.

(This replaces the older `SUM SHA-2 256`.)

#### Partition number

Format: `PARTID `, `u64`.

Deprecated; ignored if encountered.

#### Classification range

Format: `CSF`, 4-byte identifier, `u32`, `u32`.

Deprecated; ignored if encountered.


Commit meta
=======

This is something common to both snapshot files and commits (see respective
sections for usage). It defines commit metadata: timestamp, commit number,
extra core data, and "extra metadata" (user-defined).

*   an `i64` (eight byte signed) UNIX timestamp (the number of non-leap seconds
    since January 1, 1970 0:00:00 UTC) of the time the commit was made
*   legacy versions (up to 2016 03 10): `CNUM` (commit number), else:
*   `F`, a u8 (extension length), two bytes of extension flags
*   a `u32` (four byte) number, which is
    the commit number (max parent number + 1; not guaranteed unique)
*   extension data (length is previous u8 in 8 byte clusters for a maximum of
    8 × 256 = 2048 bytes); extension flags define contents,
    data is considered inessential but features may be essential
*   `XM`
*   two bytes; typically these are zero-bytes (ignore data) or `TT` (extra
    metadata is UTF-8 text); other values may be introduced in the future
*   a `u32` (four bytes unsigned) number; this is the length of the extra
    metadata below
*   Extra metadata: length is defined above; section is zero-padded to a
    16-byte boundary. Generally it is safe to ignore this data, but users may
    store extra things here (e.g. author and comment).

## Extension flags

The file format is designed to allow extensions such that (a) new software
versions can easily read old files and (b) file format extensions do not cause
unnecessary breakage with old software versions. This necessates that old
software can read new formats, determine whether it is able to read all
essential features, and skip inessential additions.

Extension flags are provided for this purpose: they are pairs of bits,
the first of which indicates that an extension is active and the second
that the extension is essential (see terminology above). If both are zero (00),
the extension is inactive or undefined, if only the first is set (10), the
extension is active but not essential; either way software not familiar with
the extension can ignore it. If the second bit is set (i.e. 01 or 11), the
extension is essential and software unfamiliar with this extension should
either abort immediately or continue with precautions (warn that future
errors may be caused by an unknown extension and usually not save new commits
or snapshots or do anything important with the data).

Flags are encoded in unsigned integers, ordered using the least significant
bits first, and numbered by the bit-shift to bring the low-bit down to
least-significant position (e.g. in bit-pattern `00101100`, the extensions with
numbers 2 and 4 are active, and and extension 2 is essential).

The following extensions are defined:

*   0: "reclassify"; deprecated and ignored

Flags are inherited by child commits (even if unknown) unless explicitly
un-set. Merge commits use the binary *or* of their parent commit's flags.
Extension data (following the flags) is not inherited.


Snapshot files
========

A snapshot file consists of a header (described above) and a snapshot, as
follows.

Snapshot
------------

Data is written as follows:

*   `SNAPSH` (section identifier), a byte (u8) indicating the number of
    parents, `U` (8 bytes total)
*   commit metadata (see above)
*   for each parent (see `SNAPSH` above), its state sum; length depends on
    checksum algorithm
*   `ELEMENTS` (section identifier)
*   number of elements as a u64

Per-element data (in any order):

*   `ELEMENT` to mark section (pad to 8 bytes with zero)
*   element identifier (u64)
*   `BYTES` (padded to 8) to mark data section and format (byte stream)
*   length of byte stream (u64)
*   data (byte stream), padded to the next 16-byte boundary
*   checksum

Memory of moved elements; this section is deprecated and unsupported.

*   `ELTMOVES` to mark section
*   number of records (u64)
*   for each record, (u64, u64)

Finally:

*   `STATESUM` (section identifier)
*   number of elements as u64 (repeated, mostly for alignment)
*   state checksum (doubles as an identifier)
*   checksum of data as written in file


Log files
======

A log file consists of a header (described above), a log section identifier,
and any number of logs.

The section identifier is: `COMMIT LOG      ` (16 bytes). This follows the
header and is followed by any number of commits (there is no counter since
files support simple extension). Commits are weakly ordered in that a commit
must come after commit(s) for its parent state(s).


Commits
----------

Normal commits start with the identifier `COMMIT` (6 bytes).
Merge commits start with the identifier `MERGE`, followed by a `u8` (unsigned
byte) indicating the number of parents (must be at least two); again 6 bytes.

This is followed by:

*   `\x00U` (2 bytes: zero U), indicating that a UTC UNIX timestamp follows
*   commit metadata (see above)
*   for each parent (one for `COMMIT`, two or more for `MERGE`; see above), its
    state sum; length depends on checksum algorithm
*   `ELEMENTS`
*   number of changes
*   PER CHANGE DATA
*   a state checksum
*   a checksum of the commit data (from start of the commit to just before
    this checksum itself)

Note that there must be at least one parent to a commit, and the first parent
is the one to which this commit is the "diff" (can be patched onto to derive
the commit's state).

### Per change data

Where "PER CHANGE DATA" is written above, a sequence of element-specific
sections appears. Elements may appear in any order. The syntax for each
element is:

*   section identifier: `ELT ` followed by one of
    
    *   `DEL` (delete)
    *   `INS` (insert with new element id)
    *   `REPL` (replace an existing element with new data)
    *   `MOV`, `MOVO`: deprecated and unsupported
    *   (TODO) `PATC` (patch an existing element)
*   element identifier (partition specific, u64)

Contents now depend on the previous identifier:

*   `DEL`: no extra content
*   `INS`: identifier `ELT DATA`, data length (u64), data (padded to 16-byte
    boundary with \\x00), data checksum (used to calculate the state sum)
*   `REPL`: contents is identical to `INS`, but `INS` is only allowed when the
    element identifier was free while `REPL` is only allowed when the
    identifier pointed to an element in the previous state.
*   `MOVO` and `MOV`: identifier `NEW ELT` (pad to 8 bytes), element identifier
    (u64)

