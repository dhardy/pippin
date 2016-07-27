// Subject to the ISC licence (LICENSE-ISC.txt).

extern crate rand;
extern crate pippin;
extern crate pippin_app_tests;

use std::cmp::min;
use std::cell::Cell;
use rand::Rng;
use rand::distributions::{IndependentSample, LogNormal};
use pippin::{MutStateT, Repository};
use pippin::fileio::RepoFileIO;
use pippin::commit::MakeMeta;
use pippin::error::ElementOp;
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
impl MakeMeta for RepeatableMeta {
    // add one hour
    fn make_timestamp(&self) -> i64 {
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
        let gen = GeneratorEnum::new_random(&mut rng);
        generate(&mut state, &mut rng, 50, &gen).expect("generate");
        repo.merge_in(state, Some(&meta_gen)).expect("merge");
        repo.write_all(false).expect("write");
    }
    
    //TODO: compare result with something
    tmp_dir.release();  // TODO: we might need to keep it
}

fn generate<R: Rng>(state: &mut MutStateT<Sequence>, rng: &mut R,
    num: usize, generator: &Generator) -> Result<(), ElementOp>
{
    let len_range = LogNormal::new(1., 2.);
    let max_len = 1_000;
    for _ in 0..num {
        let len = min(len_range.ind_sample(rng) as usize, max_len);
        let seq = generator.generate(len).into();
        state.insert_initial(rng.gen::<u32>() & 0xFF_FFFF, seq).expect("insert element");
    }
    Ok(())
}
