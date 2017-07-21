<!-- This Source Code Form is subject to the terms of the Mozilla Public
   - License, v. 2.0. If a copy of the MPL was not distributed with this
   - file, You can obtain one at http://mozilla.org/MPL/2.0/. -->


Requirements of DB
=============


Key features
--------------

*   Persistant, highly fault-tolerant data storage (full checksumming, corruption only causes
    local data loss, if any)
*   Backup-friendly files: large ones don't change, small ones grow only (if possible?),
    keep total number low
*   Store history of changes
*   Allow pruning of old history
*   Distributed synchronisation (like DVCS, e.g. git, Mercurial)
*   Ability to scale storage beyond RAM availability, and keep RAM usage low
*   Flexible indices(?)
*   Server functionality: clone (with or without all history), fetch changes, push changes;
    possibly also direct reads of individual items


Use-cases
----------

1.  Read all data, (possibly) make a few modifications, write changes
2.  Create new, add lots of data, write
3.  Read specific subset of data
4.  Read only a few uncorrelated ("random") entries
5.  Open with low memory usage, read something, free, repeat (keeping low memory usage)
