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
use rand::distributions::{IndependentSample, LogNormal};

use pippin::{PartId, Partition, StateRead, StateWrite, Result};
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
    flag_partition: Option<u64>,
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
    
    let result = run(Path::new(&args.arg_PATH), args.flag_partition,
            args.flag_list, args.flag_generate, args.flag_create,
            args.flag_snapshot, repetitions);
    if let Err(e) = result {
        println!("Error: {}", e);
        exit(1);
    }
}

// part_num: None for repo mode, Some(PN) for partition mode, where PN may be
// 0 (auto mode) or a partition number
fn run(path: &Path, part_num: Option<u64>,
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
                state.insert(seq).expect("insert element");
            }
            println!("Generated {} sequences; longest length {}, average {}",
                    num, longest, (total as f64) / (num as f64));
        } else {}
    ;
    
    if let Some(pn) = part_num {
        let mut part = if create {
            // — Create partition —
            // On creation we need a number; 0 here means "default":
            let part_id = PartId::from_num(if pn == 0 { 1 } else { pn });
            let io = Box::new(fileio::PartFileIO::new_empty(part_id, path.join("seqdb")));
            Partition::<Sequence>::create(io, "sequences db", None, None)?
        } else {
            // — Open partition —
            let part_id = if pn != 0 { Some(PartId::from_num(pn)) } else { None };
            let io = Box::new(discover::part_from_path(path, part_id)?);
            let mut part = Partition::<Sequence>::open(io)?;
            part.load_latest(None, None)?;
            part
        };
        
        if part.merge_required() {
            part.merge(&merge_solver, true, None)?;
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
            part.push_state(state, None)?;
            part.write(false, None)?;
        }
        
        if snapshot {
            part.write_snapshot(None)?;
        }
    } else {
        let discover = discover::repo_from_path(path)?;
        let rt = SeqRepo::new(discover);
        
        let mut repo = if create {
            // — Create repository —
            Repository::create(rt, "sequences db", None)?
        } else {
            // — Open repository —
            let mut repo = Repository::open(rt)?;
            repo.load_latest(None)?;
            repo
        };
        
        if repo.merge_required() {
            repo.merge(&merge_solver, true, None)?;
        }
        
        if let Some(_num) = list_n {
            println!("-l / --list option only works in single-partition (-p) mode for now");
            //TODO: how do we iterate over all elements of a repo?
        }
        
        for _ in 0..repetitions {
            let mut state = repo.clone_state()?;
            println!("Found {} partitions with {} elements", state.num_parts(), state.num_avail());
            generate(&mut state);
            println!("Done modifying state");
            repo.merge_in(state, None)?;
            repo.write_all(false)?;
        }
        
        if snapshot {
            repo.write_snapshot_all()?;
        }
    }
    
    Ok(())
}
