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
