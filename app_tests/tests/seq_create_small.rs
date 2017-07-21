// Subject to the ISC licence (LICENSE-ISC.txt).

extern crate rand;
extern crate pippin;
extern crate pippin_app_tests;

use std::cmp::min;
use rand::Rng;
use rand::distributions::{IndependentSample, LogNormal};
use pippin::pip::*;
use pippin_app_tests::util;
use pippin_app_tests::seq::*;


// —————  tests  —————
#[test]
fn create() {
    let mut tmp_dir = util::mk_temp_dir("seq_small");
    
    // make some repeatable generator
    let mut rng = util::mk_rng(916118);
    
    let io = RepoFileIO::new(tmp_dir.to_path_buf().join("data"));
    let control = SeqControl::new(Box::new(io));
    let mut repo = Partition::create(control, "seq_create_small").expect("repo create");
    
    for _ in 0..5 {
        let mut state = repo.tip().expect("tip").clone_mut();
        let len_range = LogNormal::new(1., 2.);
        let max_len = 1_000;
        for _ in 0..50 {
            let gen = GeneratorEnum::new_random(&mut rng);
            let len = min(len_range.ind_sample(&mut rng) as usize, max_len);
            let seq = gen.generate(len).into();
            // Note: only reason for the weird number generation is to maintain previous results
            let initial = EltId::from((rng.gen::<u32>() & 0xFF_FFFF) as u64);
            let id = state.free_id_near(initial).expect("find free id");
            state.insert(id, seq).expect("insert element");
        }
        repo.push_state(state).expect("merge");
        repo.write_full().expect("write");
    }
    
    // We do two types of check. Because our "random" number generator is
    // deterministic, we know what elements to expect. And we also know what
    // data files to expect.
    
    let tip_key = repo.tip_key().expect("tip_key");
    // This is what we found before. Check consistency rather than correctness.
    // We should have one partition, so "each" iteration is the same.
    assert_eq!(tip_key.as_string(false),
        "72A7CFD83C8A1F33723AD863A5CE3DB9BFB73574CAE9C5130F543390AC1FF998");
    
    let tip = repo.tip().expect("tip");
    assert_eq!(tip.num_avail(), 250);
    
    assert_eq!(*tip.get(3112864.into()).expect("get 3112864"), Sequence::from(vec![]));
    assert_eq!(*tip.get(7907964.into()).expect("get 7907964"),
            Sequence::from(vec![0.0000000000000000000000000000000000000000000000000000000000019783199897478986]));
    assert_eq!(tip.get(13218343.into()).expect("get 13218343").len(), 9);
    
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
    let repo_dir = util::get_data_dir("seq_small");
    let mut io = part_from_path(repo_dir.to_path_buf()).expect("discover");
    io.set_readonly(true);
    let control = SeqControl::new(Box::new(io));
    let mut repo = Partition::open(control, true).expect("open");
    
    // make some repeatable generator
    let mut rng = util::mk_rng(3168136158);
    
    let mut state = repo.tip().expect("tip").clone_mut();
    let len_range = LogNormal::new(1., 2.);
    let max_len = 1_000;
    for _ in 0..50 {
        let gen = GeneratorEnum::new_random(&mut rng);
        let len = min(len_range.ind_sample(&mut rng) as usize, max_len);
        let seq = gen.generate(len).into();
        let initial = EltId::from(rng.gen::<u64>());
        let id = state.free_id_near(initial).expect("find free id");
        state.insert(id, seq).expect("insert element");
    }
    repo.push_state(state).expect("merge");
    
    let tip_key = repo.tip_key().expect("tip_key");
    // This is what we found before. Check consistency rather than correctness.
    // We should have one partition, so "each" iteration is the same.
    assert_eq!(tip_key.as_string(false),
        "F6ACA0EDD345FE2C5C3EA1C252DF827D9C3A0EF285BCAD35B546900F82C38C82");
    let tip = repo.tip().expect("tip");
    assert_eq!(tip.num_avail(), 300);
}
