//! Command-line UI for Pippin

extern crate pippin;
extern crate rustc_serialize;
extern crate docopt;

use std::process::exit;
use std::path::PathBuf;
use std::fs;
use docopt::Docopt;
use pippin::{Repo, Element, Result};

const USAGE: &'static str = "
Pippin command-line UI. This is a demo app and should not be relied upon for
any deployment. It is not optimised for large repos and the UI may change.

Usage:
  pippincmd -f <file> stats
  pippincmd -f <file> list
  pippincmd -f <file> get  <id>
  pippincmd -f <file> create <repo-name> [--force]
  pippincmd -f <file> insert <id> <data> [--force]
  pippincmd (-h | --help)
  pippincmd version

Options:
  stats                 Show statistics for a repository loaded from <file>.
  list                  List element identifiers.
  get <id>              Get an element by its numeric identifier.
  create <repo-name>    Create a new repository.
  insert <id> <data>    Insert some element to the repo.
  
  -f --file <file>      File to load/save a repository from/to.
  -F --force            Overwrite file instead of failing.
  -h --help             Show this message.
  --version             Show version.
";

#[derive(Debug, RustcDecodable)]
struct Args {
    cmd_stats: bool,
    cmd_list: bool,
    cmd_get: bool,
    cmd_create: bool,
    cmd_insert: bool,
    flag_version: bool,
    flag_force: bool,
    flag_file: Option<String>,
    arg_repo_name: Option<String>,
    arg_id: Option<u64>,
    arg_data: Option<String>,
}

fn main() {
    let args: Args = Docopt::new(USAGE)
                            .and_then(|d| d.decode())
                            .unwrap_or_else(|e| e.exit());
    
    if args.flag_version {
        let pv = pippin::LIB_VERSION;
        println!("Pippin version: {}.{}.{}",
            (pv >> 32) & 0xFFFF,
            (pv >> 16) & 0xFFFF,
            pv & 0xFFFF);
    } else {
        match main_inner(args) {
            Ok(()) => {},
            Err(e) => {
                println!("Error: {}", e);
                exit(1);
            }
        }
    }
}

fn extract<T>(x: Option<T>, opt: &str) -> T {
    match x {
        Some(v) => v,
        None => {
            println!("Required option missing: '{}'", opt);
            exit(1);
        }
    }
}

fn main_inner(args: Args) -> Result<()> {
    let load = args.cmd_stats || args.cmd_list || args.cmd_get || args.cmd_insert;
    
    let path = PathBuf::from(extract(args.flag_file, "-f <file>"));
    //TODO: use path.is_file() when stable in libstd
    let is_file = if let Ok(m) = fs::metadata(&path) {
        m.is_file() } else { false };
    
    if load {
        if !is_file {
            println!("This isn't a file or can't be opened: {}", path.display());
            exit(1);
        }
        let mut repo = try!(Repo::load_file(&path));
        
        if args.cmd_stats {
            println!("Repo name: {}", repo.name());
            println!("Number of elements: {}", repo.num_elts());
        } else if args.cmd_list {
            println!("Repo name: {}", repo.name());
            println!("Elements:");
            for id in repo.element_ids() {
                println!("  {}", id);
            }
        } else if args.cmd_get {
            let id = extract(args.arg_id, "<id>");
            
            match repo.get_element(id) {
                None => { println!("No element {}", id); },
                Some(d) => {
                    println!("Element {}:", id);
                    println!("{}", String::from_utf8_lossy(d.data()));
                }
            }
        } else if args.cmd_insert {
            let id = extract(args.arg_id, "<id>");
            let data = extract(args.arg_data, "<data>");
            
            match repo.insert_elt(id, Element::from_vec(data.into())) {
                Ok(()) => { println!("Element {} inserted.", id); },
                Err(e) => { println!("Element {} couldn't be inserted: {}", id, e); }
            };
            
            //TODO: only if changed
            try!(repo.save_file(&path));
        }
    } else if args.cmd_create {
        let repo_name = extract(args.arg_repo_name, "<repo-name>");
        
        let repo = try!(Repo::new(repo_name));
        
        if !args.flag_force && is_file {
            println!("Already exists: {}", path.display());
            exit(1);
        }
        try!(repo.save_file(&path));
    }
    Ok(())
}
