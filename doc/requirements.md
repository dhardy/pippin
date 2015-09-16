Requirements of synchronised sets
==================

Synchronisation sets will be used for managing messages. Elements will contain:

*   tags used to organise messages (like gmail tags)
*   possibly the subject line or other short description
*   maybe keywords used for searching
*   identifiers of data blobs storing message content

1.  Needs to represent a set
2.  Within each element, partial changes need to be propagated
3.  It should be possible to partition the set, should it be impossible to load
    the whole thing into memory, with some user control over how items are
    partitioned
4.  Sufficient history to allow synchronisation must be saved. Since it may not be known when
    another copy may be branched from, some features may be required to be saved indefinitely.
5.  The system should be optimised for the case where a lot of elements exist with a lot of
    history, access to the contents of all elements in their current state is frequently needed,
    and the size of the contents of elements is small (not significantly larger than the
    history log).

