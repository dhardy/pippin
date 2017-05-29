/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Convenient bindings to Pippin types, traits, functions and constants.
//! 
//! TODO: should anything be renamed, or excluded from this list for clarity?

pub use ::LIB_VERSION;
pub use commit::{ExtraMeta, MetaFlags, CommitMeta, CommitMetaPartial, Commit, EltChange};
pub use elt::{EltId, PartId, ElementT};
pub use error::{Result, Error, ReadError, ReadErrorFormatter, ArgError, ElementOp, PatchOp,
        PathError, MatchError, TipError, MergeError, ReadOnly, UserError, RepoDivideError,
        OtherError, make_io_err};
pub use io::{DummyPartIO, PartIO, RepoIO};
pub use io::discover::{part_from_path, repo_from_path, part_num_from_name, find_part_num,
        discover_basename};
pub use io::file::{PartPaths, PartFileIO, RepoFileIO, RepoPartIter};
pub use merge::{TwoWayMerge, EltMerge, TwoWaySolver, TwoWaySolveUseA, TwoWaySolveUseB,
        TwoWaySolveUseC, TwoWaySolveFail, TwoWaySolverChain, AncestorSolver2W, RenamingSolver2W};
pub use part::{Partition, DefaultSnapshot, DefaultUserPartT, UserPartT, SnapshotPolicy, TipIter, StateItem, StateIter};
pub use repo::{Repository, RepoT, ClassifierT, ClassifyFallback, RepoState,
        DummyClassifier, PartIter, PartIterMut};
pub use state::{PartState, MutPartState, StateT, MutStateT, EltIter, EltIdIter, PartStateSumComparator};
pub use sum::{Sum, SUM_BYTES};
pub use util::{rtrim, ByteFormatter, HexFormatter};
pub use readwrite::{FileHeader, FileType, read_head, UserData};
