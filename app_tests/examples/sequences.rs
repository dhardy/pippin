// Subject to the ISC licence (LICENSE-ISC.txt).

extern crate byteorder;
extern crate rustc_serialize;
extern crate docopt;
#[macro_use(try_read)]
extern crate pippin;
extern crate rand;
#[macro_use]
extern crate log;
extern crate env_logger;
extern crate pippin_app_tests;

use std::path::{Path};
use std::process::exit;
use std::cmp::{min, max};

use docopt::Docopt;
use rand::Rng;
use rand::distributions::{IndependentSample, LogNormal};

use pippin::{PartId, Partition, StateT, MutStateT, Result};
use pippin::{discover, fileio};
use pippin::repo::Repository;
use pippin::merge::*;

use pippin_app_tests::seq::*;


// —————  main  —————
const USAGE: &'static str = "
Generates or reads a database of sequences.
In repository mode, PATH should be a directory. In single-partition mode, PATH
may be either a directory or a data file.

Usage:
  sequences [options] PATH

Options:
  -h --help             Show this message.
  -p --partition PN     Create or read only a partition number PN. Use PN=0 for
                        auto-detection but to still use single-partition mode.
  -c --create           Create a new repository
  -s --snapshot         Force creation of snapshot at end
  -g --generate NUM     Generate NUM new sequences and add to the repo.
  -R --repeat N         Repeat N times.

Note that you shouldn't try to create a partition with `-p`, then load that
partition alongside others as part of a repository; at least not without making
sure the repository name and partition number are correct.
";

#[derive(Debug, RustcDecodable)]
#[allow(non_snake_case)]
struct Args {
    arg_PATH: String,
    flag_partition: Option<u64>,
    flag_generate: Option<usize>,
    flag_create: bool,
    flag_snapshot: bool,
    flag_repeat: Option<usize>,
}

#[derive(PartialEq, Debug)]
enum Mode {
    Generate(usize),
    None
}

fn main() {
    env_logger::init().unwrap();
    
    let args: Args = Docopt::new(USAGE)
            .and_then(|d| d.decode())
            .unwrap_or_else(|e| e.exit());
    
    let mode = match args.flag_generate {
        Some(num) => Mode::Generate(num),
        None => Mode::None,
    };
    
    let repetitions = args.flag_repeat.unwrap_or(1);
    
    let result = run(Path::new(&args.arg_PATH), args.flag_partition,
            mode, args.flag_create,
            args.flag_snapshot, repetitions);
    if let Err(e) = result {
        println!("Error: {}", e);
        exit(1);
    }
}

// part_num: None for repo mode, Some(PN) for partition mode, where PN may be
// 0 (auto mode) or a partition number
fn run(path: &Path, part_num: Option<u64>, mode: Mode, create: bool,
        snapshot: bool, repetitions: usize) -> Result<()>
{
    let solver1 = AncestorSolver2W::new();
    let solver2 = RenamingSolver2W::new();
    let merge_solver = TwoWaySolverChain::new(&solver1, &solver2);
    
    let mut rng = rand::thread_rng();
    let mut generate = |state: &mut MutStateT<_>| match mode {
        Mode::Generate(num) => {
            let gen = GeneratorEnum::new_random(&mut rng);
            generate(state, &mut rng, num, &gen);
        },
        Mode::None => {},
    };
    
    if let Some(pn) = part_num {
        let mut part = if create {
            // On creation we need a number; 0 here means "default":
            let part_id = PartId::from_num(if pn == 0 { 1 } else { pn });
            let io = Box::new(fileio::PartFileIO::new_empty(part_id, path.join("seqdb")));
            try!(Partition::<Sequence>::create(io, "sequences db", None, None))
        } else {
            let part_id = if pn != 0 { Some(PartId::from_num(pn)) } else { None };
            let io = Box::new(try!(discover::part_from_path(path, part_id)));
            let mut part = try!(Partition::<Sequence>::open(io));
            try!(part.load_latest(None, None));
            part
        };
        
        if part.merge_required() {
            try!(part.merge(&merge_solver, true, None));
        }
        
        for _ in 0..repetitions {
            let mut state = {
                let tip = try!(part.tip());
                println!("Found state {}; have {} elements", tip.statesum(), tip.num_avail());
                tip.clone_mut()
            };
            generate(&mut state);
            println!("Done modifying state");
            try!(part.push_state(state, None));
            try!(part.write(false, None));
        }
        
        if snapshot {
            try!(part.write_snapshot(None));
        }
    } else {
        let discover = try!(discover::repo_from_path(path));
        let rt = SeqRepo::new(discover);
        
        let mut repo = if create {
            try!(Repository::create(rt, "sequences db", None))
        } else {
            let mut repo = try!(Repository::open(rt));
            try!(repo.load_latest(None));
            repo
        };
        
        if repo.merge_required() {
            try!(repo.merge(&merge_solver, true, None));
        }
        
        for _ in 0..repetitions {
            let mut state = try!(repo.clone_state());
            println!("Found {} partitions; with {} elements", state.num_parts(), state.num_avail());
            generate(&mut state);
            println!("Done modifying state");
            try!(repo.merge_in(state, None));
            try!(repo.write_all(false));
        }
        
        if snapshot {
            try!(repo.write_snapshot_all());
        }
    }
    
    Ok(())
}

fn generate<R: Rng>(state: &mut MutStateT<Sequence>, rng: &mut R,
    num: usize, generator: &Generator)
{
    let len_range = LogNormal::new(1., 2.);
    let max_len = 1_000;
    let mut longest = 0;
    let mut total = 0;
    for _ in 0..num {
        let len = min(len_range.ind_sample(rng) as usize, max_len);
        longest = max(longest, len);
        total += len;
        let seq = generator.generate(len).into();
        state.insert(seq).expect("insert element");
    }
    println!("Generated {} sequences; longest length {}, average {}", num, longest, (total as f64) / (num as f64));
}
