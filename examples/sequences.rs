/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

extern crate byteorder;
extern crate rustc_serialize;
extern crate docopt;
extern crate pippin;
extern crate rand;
#[macro_use]
extern crate log;
extern crate env_logger;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::exit;
use std::fs;
use std::cmp::min;

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use docopt::Docopt;
use rand::Rng;
use rand::distributions::{IndependentSample, Range, Normal, LogNormal};

use pippin::{ElementT, PartId, Partition, State};
use pippin::discover::*;
use pippin::repo::*;
use pippin::merge::*;
use pippin::error::{Result, OtherError};


// —————  Sequence type  —————
type R = f64;
const R_SIZE: usize = 8;
#[derive(PartialEq, Debug)]
struct Sequence { v: Vec<R> }

impl ElementT for Sequence {
    fn write_buf(&self, writer: &mut Write) -> Result<()> {
        for x in &self.v {
            try!(writer.write_f64::<LittleEndian>(*x));
        }
        Ok(())
    }
    fn read_buf(buf: &[u8]) -> Result<Self> {
        if buf.len() % R_SIZE != 0 {
            return OtherError::err("invalid data length for a Sequence");
        }
        let mut r: &mut &[u8] = &mut &buf[..];
        let n = buf.len() / R_SIZE;
        let mut v = Vec::with_capacity(n);
        for _ in 0..n {
            v.push(try!(r.read_f64::<LittleEndian>()));
        }
        Ok(Sequence{ v: v })
    }
}

// —————  derived types for Repo  —————
#[derive(Clone)]
struct SeqClassifier;
struct ReqRepo<R: RepoIO> {
    csf: SeqClassifier,
    io: R,
}
impl<R: RepoIO> ReqRepo<R> {
    pub fn new(r: R) -> ReqRepo<R> {
        ReqRepo { csf: SeqClassifier, io: r }
    }
}
impl ClassifierT for SeqClassifier {
    type Element = Sequence;
    fn classify(&self, _elt: &Sequence) -> Option<PartId> {
        // currently we do not partition
        Some(PartId::from_num(1))
    }
    fn fallback(&self) -> ClassifyFallback {
        ClassifyFallback::Default(PartId::from_num(1))
    }
}
impl<R: RepoIO> ClassifierT for ReqRepo<R> {
    type Element = Sequence;
    fn classify(&self, elt: &Sequence) -> Option<PartId> { self.csf.classify(elt) }
    fn fallback(&self) -> ClassifyFallback { self.csf.fallback() }
}
impl<R: RepoIO> RepoT<SeqClassifier> for ReqRepo<R> {
    fn repo_io(&mut self) -> &mut RepoIO {
        &mut self.io
    }
    fn clone_classifier(&self) -> SeqClassifier {
        self.csf.clone()
    }
    fn divide(&mut self, _class: PartId) ->
        Result<(Vec<PartId>, Vec<PartId>), RepoDivideError>
    {
        // currently we do not partition
        Err(RepoDivideError::NotSubdivisible)
    }
    fn write_buf(&self, num: PartId, writer: &mut Write) -> Result<()> {
        // currently nothing to write
        Ok(())
    }
    fn read_buf(&mut self, num: PartId, buf: &[u8]) -> Result<()> {
        // currently nothing to read
        Ok(())
    }
}


// —————  Generators  —————
trait Generator {
    fn generate(&self, n: usize) -> Vec<R>;
}
struct Arithmetic { start: R, step: R }
struct Geometric { start: R, factor: R }
struct Fibonacci { x1: R, x2: R }
struct Power { e: R }

impl Generator for Arithmetic {
    fn generate(&self, n: usize) -> Vec<R> {
        let mut v = Vec::with_capacity(n);
        let mut x = self.start;
        while v.len() < n {
            v.push(x);
            x += self.step;
        }
        v
    }
}
impl Generator for Geometric {
    fn generate(&self, n: usize) -> Vec<R> {
        let mut v = Vec::with_capacity(n);
        let mut x = self.start;
        while v.len() < n {
            v.push(x);
            x *= self.factor;
        }
        v
    }
}
impl Generator for Fibonacci {
    fn generate(&self, n: usize) -> Vec<R> {
        let mut v = Vec::with_capacity(n);
        let (mut x1, mut x2) = (self.x1, self.x2);
        while v.len() < n {
            v.push(x1);
            let x = x1 + x2;
            x1 = x2;
            x2 = x;
        }
        v
    }
}
impl Generator for Power {
    fn generate(&self, n: usize) -> Vec<R> {
        let mut v = Vec::with_capacity(n);
        let mut i: R = 0.0;
        while v.len() < n {
            v.push(i.powf(self.e));
            i += 1.0;
        }
        v
    }
}


// —————  main  —————
const USAGE: &'static str = "
Generates or reads a database of sequences.

Usage:
  sequences [options]

Options:
  -h --help             Show this message.
  -d --directory DIR    Specify the directory to read/write the repository
  -p --partition BASE   Instead of a whole repository, just use the partition
                        with basename BASE (e.g. the basename of
                        'xyz-pn1-ss0.pip' is 'xyz-pn1').
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
    flag_directory: Option<String>,
    flag_partition: Option<String>,
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
    
    let dir = PathBuf::from(match args.flag_directory {
        Some(dir) => dir,
        None => {
            println!("Error: --directory option required (use --help for usage)");
            exit(1);
        },
    });
    if fs::create_dir_all(&dir).is_err() {
        println!("Unable to create/find directory {}", dir.display());
        exit(1);
    }
    
    let mode = match args.flag_generate {
        Some(num) => Mode::Generate(num),
        None => Mode::None,
    };
    
    let repetitions = args.flag_repeat.unwrap_or(1);
    
    let result = run(&dir, args.flag_partition, mode, args.flag_create,
            args.flag_snapshot, repetitions);
    if let Err(e) = result {
        println!("Error: {}", e);
        exit(1);
    }
}

fn run(dir: &Path, part_basename: Option<String>, mode: Mode, create: bool,
        snapshot: bool, repetitions: usize) -> Result<()>
{
    let solver1 = AncestorSolver2W::new();
    let solver2 = RenamingSolver2W::new();
    let merge_solver = TwoWaySolverChain::new(&solver1, &solver2);
    
    let mut rng = rand::thread_rng();
    let mut generate = |state: &mut State<_>| match mode {
        Mode::Generate(num) => {
            match Range::new(0, 4).ind_sample(&mut rng) {
                0 => {
                    let gen = Arithmetic{
                        start: LogNormal::new(0., 100.).ind_sample(&mut rng),
                        step: Normal::new(0., 10.).ind_sample(&mut rng),
                    };
                    generate(state, &mut rng, num, &gen);
                },
                1 => {
                    let gen = Geometric{
                        start: LogNormal::new(0., 100.).ind_sample(&mut rng),
                        factor: Normal::new(0., 2.).ind_sample(&mut rng),
                    };
                    generate(state, &mut rng, num, &gen);
                },
                2 => {
                    let gen = Fibonacci{
                        x1: Normal::new(1., 1.).ind_sample(&mut rng),
                        x2: Normal::new(1., 1.).ind_sample(&mut rng),
                    };
                    generate(state, &mut rng, num, &gen);
                },
                3 => {
                    let gen = Power{
                        e: LogNormal::new(0., 1.).ind_sample(&mut rng),
                    };
                    generate(state, &mut rng, num, &gen);
                },
                _ => { panic!("invalid sample"); }
            }
        },
        Mode::None => {},
    };
    
    if let Some(basename) = part_basename {
        let io = Box::new(try!(DiscoverPartitionFiles::from_dir_basename(dir, &basename)));
            
        let mut part = if create {
            try!(Partition::<Sequence>::create(io, "sequences db"))
        } else {
            // Try to find the partition number from the file name or fall back to 1
            let part_id = io.guess_part_num().unwrap_or(PartId::from_num(1));
            let mut part = Partition::<Sequence>::open(io, part_id);
            try!(part.load(false));
            part
        };
        
        if part.merge_required() {
            try!(part.merge(&merge_solver));
        }
        
        for _ in 0..repetitions {
            let mut state = try!(part.tip()).clone_child();
            println!("Found state {}; have {} elements", state.statesum(), state.num_avail());
            generate(&mut state);
            println!("Done modifying state");
            try!(part.push_state(state));
            try!(part.write(false));
        }
        
        if snapshot {
            try!(part.write_snapshot());
        }
    } else {
        let discover = try!(DiscoverRepoFiles::from_dir(dir));
        let rt = ReqRepo::new(discover);
        
        let mut repo = if create {
            try!(Repo::create(rt, "sequences db"))
        } else {
            let mut repo = try!(Repo::open(rt));
            try!(repo.load_all(false));
            repo
        };
        
        if repo.merge_required() {
            try!(repo.merge(&merge_solver));
        }
        
        for _ in 0..repetitions {
            let mut state = try!(repo.clone_state());
            println!("Found {} partitions; with {} elements", state.num_parts(), state.num_avail());
            generate(&mut state);
            println!("Done modifying state");
            try!(repo.merge_in(state));
            try!(repo.write_all(false));
        }
        
        if snapshot {
            try!(repo.write_snapshot_all());
        }
    }
    
    Ok(())
}

fn generate<R: Rng>(state: &mut State<Sequence>, rng: &mut R,
    num: usize, generator: &Generator)
{
    let len_range = LogNormal::new(1., 2.);
    let max_len = 1_000;
    let mut longest = 0;
    let mut total = 0;
    for _ in 0..num {
        let len = min(len_range.ind_sample(rng) as usize, max_len);
        if len > longest { longest = len; }
        total += len;
        let seq = Sequence{ v: generator.generate(len) };
        state.insert(seq).expect("insert element");
    }
    println!("Generated {} sequences; longest length {}, average {}", num, longest, (total as f64) / (num as f64));
}
