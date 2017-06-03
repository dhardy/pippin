// Subject to the ISC licence (LICENSE-ISC.txt).

extern crate rand;
extern crate pippin;
extern crate pippin_app_tests;

use std::cmp::min;
use std::cell::Cell;
use rand::Rng;
use rand::distributions::{IndependentSample, LogNormal};
use pippin::{StateRead, StateWrite, Repository};
use pippin::fileio::RepoFileIO;
use pippin::commit::MakeCommitMeta;
use pippin_app_tests::util;
use pippin_app_tests::seq::*;


// We can't use the default meta-data, with a real timestamp, since we need
// to regenerate exactly the same data each time.
struct RepeatableMeta {
    time: Cell<i64>
}
impl RepeatableMeta {
    // start of year 2000
    fn new() -> Self {
        RepeatableMeta { time: Cell::new(946684800) }
    }
}
impl MakeCommitMeta for RepeatableMeta {
    // add one hour
    fn make_commit_timestamp(&self) -> i64 {
        let time = self.time.get();
        self.time.set(time + 3600);
        time
    }
}


// —————  tests  —————
#[test]
fn create() {
    let mut tmp_dir = util::mk_temp_dir("seq_small");
    
    // make some repeatable generator
    let mut rng = util::mk_rng(916118);
    let meta_gen = RepeatableMeta::new();
    
    let io = RepoFileIO::new(tmp_dir.to_path_buf());
    let rt = SeqRepo::new(io);
    let mut repo = Repository::create(rt, "seq_create_small", Some(&meta_gen)).expect("repo create");
    
    for _ in 0..5 {
        let mut state = repo.clone_state().expect("clone state");
        let len_range = LogNormal::new(1., 2.);
        let max_len = 1_000;
        for _ in 0..50 {
            let gen = GeneratorEnum::new_random(&mut rng);
            let len = min(len_range.ind_sample(&mut rng) as usize, max_len);
            let seq = gen.generate(len).into();
            state.insert_initial(rng.gen::<u32>() & 0xFF_FFFF, seq).expect("insert element");
        }
        repo.merge_in(state, Some(&meta_gen)).expect("merge");
        repo.write_all(false).expect("write");
    }
    
    // We do two types of check. Because our "random" number generator is
    // deterministic, we know what elements to expect. And we also know what
    // data files to expect.
    
    let tip = repo.clone_state().expect("clone state");   //TODO: do we need to clone?
    assert_eq!(tip.num_avail(), 250);
    for part in repo.partitions() {
        let tip = part.tip_key().expect("tip_key");
        // This is what we found before. Check consistency rather than correctness.
        // We should have one partition, so "each" iteration is the same.
        assert_eq!(tip.as_string(false),
            "5627C6820CBC498B8A6F84ECC103E4753929863B5E82DEE107382126BCE9879C");
    }
    
    assert_eq!(*tip.get(19890080.into()).expect("get 19890080"), Sequence::from(vec![]));
    assert_eq!(*tip.get(24685180.into()).expect("get 24685180"),
            Sequence::from(vec![0.0000000000000000000000000000000000000000000000000000000000019783199897478986]));
    assert_eq!(tip.get(29995559.into()).expect("get 29995559").len(), 9);
    
    let comparator = util::get_data_dir("seq_small");
    if util::paths_are_eq(&tmp_dir, &comparator).unwrap_or(false) {
        // okay
    } else {
        println!("Please check diff {} {}", comparator.display(), tmp_dir.as_ref().display());
        tmp_dir.release();  // don't delete
        assert!(false);
    }
}

#[test]
fn insert() {
    use pippin::discover;
    
    let repo_dir = util::get_data_dir("seq_small");
    let mut io = discover::repo_from_path(repo_dir.to_path_buf()).expect("discover");
    io.set_readonly(true);
    let rt = SeqRepo::new(io);
    let mut repo = Repository::open(rt).expect("open");
    repo.load_latest(None).expect("load");
    
    // make some repeatable generator
    let mut rng = util::mk_rng(3168136158);
    let meta_gen = RepeatableMeta::new();
    
    let mut state = repo.clone_state().expect("clone state");
    let len_range = LogNormal::new(1., 2.);
    let max_len = 1_000;
    for _ in 0..50 {
        let gen = GeneratorEnum::new_random(&mut rng);
        let len = min(len_range.ind_sample(&mut rng) as usize, max_len);
        let seq = gen.generate(len).into();
        state.insert_initial(rng.gen::<u32>() & 0xFF_FFFF, seq).expect("insert element");
    }
    repo.merge_in(state, Some(&meta_gen)).expect("merge");
    
    let tip = repo.clone_state().expect("clone state");   //TODO: do we need to clone?
    assert_eq!(tip.num_avail(), 300);
    for part in repo.partitions() {
        let tip = part.tip_key().expect("tip_key");
        // This is what we found before. Check consistency rather than correctness.
        // We should have one partition, so "each" iteration is the same.
        assert_eq!(tip.as_string(false),
            "1374C065686236F33C7A1B73B1CE7577678DDB81753C119E2B89063A961CF2A1");
    }
}
