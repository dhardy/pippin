// Subject to the ISC licence (LICENSE-ISC.txt).

extern crate rand;
extern crate pippin;
extern crate pippin_app_tests;

use std::cmp::min;
use rand::Rng;
use rand::distributions::{IndependentSample, LogNormal};
use pippin::{MutStateT, Repository};
use pippin::fileio::RepoFileIO;
use pippin_app_tests::util;
use pippin_app_tests::seq::*;


// —————  tests  —————
#[test]
fn create() {
    let mut tmp_dir = util::mk_temp_dir("seq_small");
    
    let mut rng = util::mk_rng(916118);
    let mut generate = |state: &mut MutStateT<_>| {
        let gen = GeneratorEnum::new_random(&mut rng);
        generate(state, &mut rng, 50, &gen);
    };
    
    let io = RepoFileIO::new(tmp_dir.to_path_buf());
    let rt = SeqRepo::new(io);
    let mut repo = Repository::create(rt, "seq_create_small").expect("repo create");
    
    for _ in 0..5 {
        let mut state = repo.clone_state().expect("clone state");
        generate(&mut state);
        repo.merge_in(state, None).expect("merge");
        repo.write_all(false).expect("write");
    }
    
    //TODO: compare result with something
    tmp_dir.release();  // TODO: we might need to keep it
}

fn generate<R: Rng>(state: &mut MutStateT<Sequence>, rng: &mut R,
    num: usize, generator: &Generator)
{
    let len_range = LogNormal::new(1., 2.);
    let max_len = 1_000;
    for _ in 0..num {
        let len = min(len_range.ind_sample(rng) as usize, max_len);
        let seq = generator.generate(len).into();
        state.insert(seq).expect("insert element");
    }
}
