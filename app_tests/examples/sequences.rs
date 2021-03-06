// Subject to the ISC licence (LICENSE-ISC.txt).

extern crate byteorder;
extern crate rustc_serialize;
extern crate docopt;
extern crate pippin;
extern crate rand;
extern crate env_logger;
extern crate pippin_app_tests;

use std::path::{Path};
use std::process::exit;
use std::cmp::{min, max};

use docopt::Docopt;
use rand::distributions::{IndependentSample, LogNormal};

use pippin::pip::*;
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
  -c --create           Create a new repository
  -s --snapshot         Force creation of snapshot at end
  -l --list NUM         List NUM entries (in random order)
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
    flag_list: Option<usize>,
    flag_generate: Option<usize>,
    flag_create: bool,
    flag_snapshot: bool,
    flag_repeat: Option<usize>,
}

fn main() {
    env_logger::init().unwrap();
    
    let args: Args = Docopt::new(USAGE)
            .and_then(|d| d.decode())
            .unwrap_or_else(|e| e.exit());
    
    let repetitions = args.flag_repeat.unwrap_or(1);
    
    let result = run(Path::new(&args.arg_PATH),
            args.flag_list, args.flag_generate, args.flag_create,
            args.flag_snapshot, repetitions);
    if let Err(e) = result {
        println!("Error: {}", e);
        exit(1);
    }
}

fn run(path: &Path,
         list_n: Option<usize>, generate_n: Option<usize>, create: bool,
        snapshot: bool, repetitions: usize) -> Result<()>
{
    let solver1 = AncestorSolver2W::new();
    let solver2 = RenamingSolver2W::new();
    let merge_solver = TwoWaySolverChain::new(&solver1, &solver2);
    
    let mut rng = rand::thread_rng();
    let mut generate = |state: &mut StateWrite<_>|
            if let Some(num) = generate_n
        {
            let len_range = LogNormal::new(1., 2.);
            let max_len = 1_000;
            let mut longest = 0;
            let mut total = 0;
            for _ in 0..num {
                let gen = GeneratorEnum::new_random(&mut rng);
                let len = min(len_range.ind_sample(&mut rng) as usize, max_len);
                longest = max(longest, len);
                total += len;
                let seq = gen.generate(len).into();
                state.insert_new(seq).expect("insert element");
            }
            println!("Generated {} sequences; longest length {}, average {}",
                    num, longest, (total as f64) / (num as f64));
        } else {}
    ;
    
    let mut part = if create {
        // — Create partition —
        let io = RepoFileIO::new(path.join("seqdb"));
        let control = SeqControl::new(Box::new(io));
        Partition::create(control, "sequences db")?
    } else {
        // — Open partition —
        let io = part_from_path(path)?;
        let control = SeqControl::new(Box::new(io));
        Partition::<SeqControl>::open(control, true)?
    };
    
    if part.merge_required() {
        part.merge(&merge_solver, true)?;
    }
    
    if let Some(num) = list_n {
        let tip = part.tip()?;
        for (id, ref elt) in tip.elts_iter().take(num) {
            println!("Element {}: {:?}" , id, *elt);
        }
    }
    
    for _ in 0..repetitions {
        let mut state = {
            let tip = part.tip()?;
            println!("Found state {}; have {} elements", tip.statesum(), tip.num_avail());
            tip.clone_mut()
        };
        generate(&mut state);
        println!("Done modifying state");
        part.push_state(state)?;
        part.write_full()?;
    }
    
    if snapshot {
        part.write_snapshot()?;
    }
    
    Ok(())
}
