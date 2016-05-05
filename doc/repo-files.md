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


Partition file names
---------------

A 'partition' does not need to be part of a repository, but can also be used
on its own (excuse the misleading name). Either way, its state and history is
recorded via one or more snapshot files and any number of log files, each
associated with one snapshot file.

All partition files should be in the same directory.

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

Sometimes a partition's files are found via a *prefix* which is a path relative
to the repository's root directory followed by `BASENAME` and `-`; for example
if the above addressbook files are in a subdirectory `a`, the prefix would be
`a/addressbook-`.


Repositories
-----------------

A repository is a set of (at least one) partition(s). It has no "master file"
or any other storage outside of the partition files.

File names are as set out above for partitions though usually `BASENAME` ends
with `pnN` where `N` is a partition number (this avoids the need to read a
partition's file to find the partition number).

For example, a repository could have the following files:

    inbox/pn1-ss1.pip
    inbox/pn1-ss2.pip
    inbox/pn1-ss2-cl1.piplog
    inbox/pn1-ss2-cl2.piplog
    inbox/pn1-ss3.pip
    inbox/pn1-ss3-cl1.piplog
    archives/2015-pn6-ss1.pip
    archives/2015-pn6-ss2.pip
    archives/2015-pn6-ss2-cl1.piplog
    archives/2016-pn22-ss1.pip
    archives/2016-pn22-ss1-cl1.piplog
    archives/2016-pn22-ss1-cl2.piplog
    archives/2016-pn22-ss1-cl3.piplog

Repository file discovery should start from a top directory and optionally work
recursively. If not recursive, only `*.pip` and `*.piplog` files in the top
directory will be discovered; if recursive sub-directories will also be
checked. Partition files may be in any sub-directory *however* for each partition,
all files must be in the same directory. If this is not the case discovery may
fail or continue while warning that some files may be missed.
