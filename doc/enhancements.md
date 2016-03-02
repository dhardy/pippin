Potential enhancements
======================

User-selectable checksum size
-----------------------------

Currently size is fixed, since the `Sum` type really needs to be of fixed size.

*However*, smaller-than-standard sizes could be supported by not using the whole
of `Sum`'s memory when reading/writing/comparing. This means reading files with longer
checksums than are currently selected would be possible; reading files with shorter
checksums would presumably trigger an error (since it doesn't meet the current
security criteria), though supporting shorter sums could be possible.


Handling corrupt data
---------------------

There are a few things that can be done when corrupt data is detected: continue
anyway (maybe with a warning or marking some item as corrupt), retrieve whatever
is still unaffected, revert corrupt elements to a previous version (possibly also
merging changes), recover data stored reduntantly from another source.

Question: if corruption is spotted, should we immediately drop to a read-only
or pedantic mode and not allow many operations until data is repaired?

Question: should we warn about things we can repair immediately?

This is here (not in an issue) because there are not yet any test-cases to handle
or any motivation.
