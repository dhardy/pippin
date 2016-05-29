<!-- This Source Code Form is subject to the terms of the Mozilla Public
   - License, v. 2.0. If a copy of the MPL was not distributed with this
   - file, You can obtain one at http://mozilla.org/MPL/2.0/. -->

Log files
==========


Writing new commits
------------------

When new commits are written:

*   existing data must not be corrupted, which means a log file must not be
    modified unless a replicate exists
*   log files should not be written from scratch unnecessarily

Possible methods:

1.  Remember all commits since the last snapshot. On write-to-disk, make a new
    commit log containing all these. Delete redundant logs another time.
2.  Variant of 1: when loading, write a new commit log. When writing new
    commits to disk write to this log (so there is a chance of corrupting
    commits from the same session).
3.  Variant of 1: on first write-to-disk, make a new commit log. On the second
    write, create another log file. Following this, alternate between the two
    logs (also writing commits not yet in that log file).

These options *only* write to log files created in the current session. Why?
Because this way (providing other clients do the same) there is no chance of
changes conflicting with changes from another client. But maybe this isn't
necessary.

**Atomic writes:** format a commit in memory, then append atomically. I think
on some systems such writes are guaranteed not to conflict with writes from
other processes (but may interleave). If divergent states are created by
commits in a single log, this doesn't matter (merge will happen on reading).
This may or may not be reliable; [according to this report there are length
restrictions which make this unsuitable](http://stackoverflow.com/questions/1154446/is-file-append-atomic-in-unix),
however [these limits may be attributable to other language's buffering
features](https://github.com/rust-lang/rfcs/pull/1252#issuecomment-153290345).

**Verifying writes:** don't assume that simultaneous writes can't clash, but
verify the commits just written by re-reading the relevant part of the file.

**Partial logs:** as above, use new files for each session, but don't re-write
old content (at least, not always). Advantage: less writing. Disadvantage: a
major prolification of log files.

Using the above, we have some more possibilities:

4.  Select the lowest-numbered existing log which does not contain any commits
    not also written elsewhere; if there is none then create a new log. Open in
    apend mode for atomic writes. Write all existing commits not already in the
    log, then any new commits. Write each commit as a single write so that it
    is appended atomically.
5.  Variant of (4), but every so often (number of commits? timeout?) close the
    file, re-read it to verify its contents, and select a new log file
    according to the same algorithm.


Commit size
-----------

The above write algorithms do not place any hard limits on commit size. Still,
reducing the number of commits would be good for performance and avoiding
*large* commits (for some definition of *large*) *may* be a good idea.


Commit log clean-up
----------------------

*If* many log files get created (see above write policies), a deletion policy
is "needed". Assume each log file has a number.

1.  On load, read *all* logs and delete any which are entirely redundant with
    some higher-numbered log-file. Issue: there is no guarantee that a process
    is not still writing to one of these files.
2.  As above, but set some "stale" age. Only delete "stale" files. Don't ever
    write new commits to a "stale" file. Issue: there is no upper-bound on how
    long a commit may take to write. Issue: time stamps may not be reliable
    (maybe "creation time" metadata is okay?).
3.  As (1) but write the owning process PID to the log. Not okay across
    network file systems; not portable?
4.  Some time after a new snapshot is created, go back and write a unified log
    and delete old log files. Same issues as (2), plus cannot limit log files
    except by creating new snapshots (which also have their drawbacks).

No option is perfect. Perhaps (2) with a generous bound on write time assumed
(e.g. only delete files 24 hours after they become stale) is acceptable.
