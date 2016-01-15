Repository disk layout
==============

The file [file-format.md]() describes the contents of files; this document
describes the files present in a repository.

It should be noted that adhering to the naming scheme set out in this document
is not strictly required since a user can supply custom "I/O provider" trait
objects which only need provide access to "file-like" data streams somewhere.

We consider all paths relative to the repository's root directory.


Potential changes
-----------------------

The number section could change, e.g. to `p2-s5-l1` instead of `pn2-ss5-cl1`,
or even lose the `-` separators.

The partition number part (`pnN`) is present as a compromise, letting the
software more easily discover partitions and their base-names (`BASENAME` part)
without requiring that the user-provided part of the software be able to guess
or remember (via the header in every snapshot) relationships between numbers
and base-names. It is not strictly necessary and could be removed.


Partitions
---------------

A 'partition' does not need to be part of a repository, but can also be used
on its own (excuse the misleading name). Either way, its state and history is
recorded via one or more snapshot files and any number of log files, each
associated with one snapshot file.

Snapshot file names should take the form:

    BASENAMEssS.pip

and commit log file names:

    BASENAMEssS-clL.piplog

where `BASENAME` can be any substring of a path (including `/` path separators)
or nothing at all, and `S` and `L` are numbers (both non-negative integers
without leading zeros).

For example, `BASENAME` might be `addressbook-` leading to file names like

    addressbook-ss1.pip
    addressbook-ss1-cl1.piplog
    addressbook-ss1-cl2.piplog
    addressbook-ss2.pip
    addressbook-ss2-cl1.piplog


Repositories
-----------------

A repository is a set of (at least one) partition(s). It has no "master file"
or any other storage outside of the partition files.

File names are as set out above for partitions except that `BASENAME` must end
`pnN` where `N` is a partition number.

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
