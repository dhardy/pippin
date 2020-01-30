<!-- This Source Code Form is subject to the terms of the Mozilla Public
   - License, v. 2.0. If a copy of the MPL was not distributed with this
   - file, You can obtain one at http://mozilla.org/MPL/2.0/. -->

Repository disk layout
==============

The file [file-format.md]() describes the contents of files; this document
describes the files present in a repository.

It should be noted that adhering to the naming scheme set out in this document
is not strictly required since a user can supply custom "I/O provider" trait
objects which only need provide access to "file-like" data streams somewhere.

We consider all paths relative to the repository's root directory.


File names
---------------

A repository's state and history is recorded via one or more snapshot files and any number of log
files, each associated with one snapshot file.

All files should be in the same directory.

Snapshot file names should take one of the forms:

    ssS.pip
    BASENAME-ssS.pip

and commit log file names:

    ssS-clL.piplog
    BASENAME-ssS-clL.piplog

where `BASENAME` can be any substring of a path (including `/` path separators)
or nothing at all, and `S` and `L` are numbers (both non-negative integers
without leading zeros).

For example, `BASENAME` might be `addressbook` leading to file names like

    addressbook-ss1.pip
    addressbook-ss1-cl1.piplog
    addressbook-ss1-cl2.piplog
    addressbook-ss2.pip
    addressbook-ss2-cl1.piplog

`BASENAME` may end with `pnN` as with repositories (below), e.g. `example-pn5`.
