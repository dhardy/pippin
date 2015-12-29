Pippin tickets
=======

New tickets
--------------

Each file should be named like `NNNN-some-name` where `NNNN` is a four-digit
number uniquely identifying the ticket.

Tickets may reference points in the code: the ticket number should be prepended
by `#` and used as a tag, e.g. `#0123`. In this case the tag should appear at
the start of the first line of the ticket file as well as at points of
interest in the code.

The first line should in any case be a short description.


Closing tickets
------------------

Before closing, if the ticket number is at the start of the file as a hash-tag
(e.g. `#0123`), then the code should be searched for this tag. Each mention
should be checked.

Closed tickets should be moved to the `closed` directory.
