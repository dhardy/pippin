//! Command-line UI for Pippin
#![feature(path_ext)]
#![feature(box_syntax)]

extern crate pippin;
extern crate rustc_serialize;
extern crate docopt;

use std::process::exit;
use std::path::PathBuf;
use std::fs;
use std::fs::PathExt;
use std::io::ErrorKind;
use docopt::Docopt;
use pippin::{Repo, Element, Result, Error};
use pippin::{DiscoverPartitionFiles, Partition, PartitionIO};

const USAGE: &'static str = "
Pippin command-line UI. This program is designed to demonstrate Pippin's
capabilities and to allow direct inspection of Pippin files. It is not intended
for automated usage and the UI may be subject to changes.

Usage:
  pippincmd -n NAME FILE
  pippincmd [-P] FILE...
  pippincmd [-p PART] [-S] [-C] FILE...
  pippincmd [-p PART] [-c COMMIT] [-s] [-E | -g ELT | -e ELT | -v ELT | -d ELT] FILE...
  pippincmd --help | --version

Options:
  -n --new NAME         Create a new partition with name NAME. A default state
                        (no elements) is created.
  -s --snapshot         Force writing of a snapshot after loading and applying
                        any changes, unless the state already has a snapshot.
  
  -P --partitions       List all partitions loaded
  -p --partition PART   Select partition PART
  -S --snapshots        List all snapshots loaded
  -C --commits          List all commits loaded (from snapshots and logs)
  -c --commit COMMIT    Select commit COMMIT. If not specified, most operations
                        on commits will use the head (i.e. the latest state).
  -E --elements         List all elements
  -g --get ELT          Read the contents of an element to standard output.
  -e --edit ELT         Write an element to a temporary file and invoke the
                        editor $EDITOR on that file, saving changes afterwards.
                        If ELT does not exist, it will be created.
  -v --visual ELT       Like --edit, but use $VISUAL.
  -d --delete ELT       Remove an element.
  
  -h --help             Show this message.
  --version             Show version.
";

#[derive(Debug, RustcDecodable)]
struct Args {
    arg_FILE: Vec<String>,
    flag_new: Option<String>,
    flag_snapshot: bool,
    flag_partitions: bool,
    flag_partition: Option<String>,
    flag_snapshots: bool,
    flag_commits: bool,
    flag_commit: Option<String>,
    flag_elements: bool,
    flag_get: Option<String>,
    flag_edit: Option<String>,
    flag_visual: Option<String>,
    flag_delete: Option<String>,
    flag_help: bool,
    flag_version: bool,
}

#[derive(Debug)]
enum PartitionOp {
    List(bool /*list snapshot files?*/, bool /*list log files?*/),
    ListElts,
    EltGet(String),
    EltEdit(String, Editor),
    EltDelete(String),
}
#[derive(Debug)]
enum Editor { Cmd, Visual }
#[derive(Debug)]
enum Operation {
    NewPartition(String),
    ListPartitions,
    OnPartition(PartitionOp),
    /// Default operation: print out a few statistics or something
    Default,
}

fn main() {
    let args: Args = Docopt::new(USAGE)
                            .and_then(|d| d.decode())
                            .unwrap_or_else(|e| e.exit());
    
    if args.flag_help {
        println!("{}", USAGE);
    } else if args.flag_version {
        let pv = pippin::LIB_VERSION;
        println!("Pippin version: {}.{}.{}",
            (pv >> 32) & 0xFFFF,
            (pv >> 16) & 0xFFFF,
            pv & 0xFFFF);
        println!("pippincmd version: 1.0.0");
    } else {
        // Rely on docopt to spot invalid conflicting flags
        let op = if let Some(name) = args.flag_new {
                Operation::NewPartition(name)
            } else if args.flag_partitions {
                Operation::ListPartitions
            } else if args.flag_snapshots || args.flag_commits {
                Operation::OnPartition(PartitionOp::List(args.flag_snapshots, args.flag_commits))
            } else if args.flag_elements {
                Operation::OnPartition(PartitionOp::ListElts)
            } else if let Some(elt) = args.flag_get {
                Operation::OnPartition(PartitionOp::EltGet(elt))
            } else if let Some(elt) = args.flag_edit {
                Operation::OnPartition(PartitionOp::EltEdit(elt, Editor::Cmd))
            } else if let Some(elt) = args.flag_visual {
                Operation::OnPartition(PartitionOp::EltEdit(elt, Editor::Visual))
            } else if let Some(elt) = args.flag_delete {
                Operation::OnPartition(PartitionOp::EltDelete(elt))
            } else {
                Operation::Default
            };
        match inner(args.arg_FILE, op, args.flag_partition, args.flag_commit, args.flag_snapshot) {
            Ok(()) => {},
            Err(e) => {
                println!("Error: {}", e);
                exit(1);
            }
        }
    }
}

fn inner(files: Vec<String>, op: Operation, part: Option<String>,
    commit: Option<String>, snapshot: bool) -> Result<()>
{
    let paths: Vec<PathBuf> = files.into_iter().map(|f| PathBuf::from(f)).collect();
    
    match op {
        Operation::NewPartition(name) => {
            println!("Creating new partition: {}", name);
            assert_eq!(paths.len(), 1);
            assert_eq!(part, None);
            assert_eq!(commit, None);
            //TODO: validate filename
            let path = paths.into_iter().next().unwrap();
            println!("Initial snapshot: {}", path.display());
            if path.exists() {
                return Err(Error::io(ErrorKind::AlreadyExists, "snapshot file already exists"));
            }
            
            let repo = try!(Repo::new(name));
            repo.save_file(&path)
        },
        Operation::ListPartitions => {
            println!("Multi-partition functionality not yet available");
            Ok(())
        },
        Operation::OnPartition(part_op) => {
            println!("Scanning files ...");
            //TODO: verify all files belong to the same partition `part`
            let discover = try!(DiscoverPartitionFiles::from_paths(paths));
            
            if let PartitionOp::List(list_snapshots, list_logs) = part_op {
                println!("ss_len: {}", discover.ss_len());
                for i in 0..discover.ss_len() {
                    if list_snapshots {
                        if let Some(p) = discover.get_ss_path(i) {
                            println!("Snapshot {:4}: {}", i, p.display());
                        }
                    }
                    for j in 0..discover.ss_cl_len(if list_logs {i} else {0}) {
                        if let Some(p) = discover.get_cl_path(i, j) {
                            println!("Snapshot {:4} log {:4}: {}", i, j, p.display());
                        }
                    }
                }
                Ok(())
            } else {
                let mut part = Partition::create(box discover);
                if let Some(_) = commit {
                    println!("TODO: no support yet for specified commits; using latest state instead");
                }
                try!(part.load_latest());
                //TODO merge operation
                
                {
                    let state = try!(part.tip());
                    match part_op {
                        PartitionOp::List(_,_) => { panic!("possibility already eliminated"); },
                        PartitionOp::ListElts => {
                            println!("Elements:");
                            for id in state.elt_ids() {
                                println!("  {}", id);
                            }
                        },
                        PartitionOp::EltGet(elt) => {
                            let id: u64 = try!(elt.parse());
                            match state.get_elt(id) {
                                None => { println!("No element {}", id); },
                                Some(d) => {
                                    println!("Element {}:", id);
                                    println!("{}", String::from_utf8_lossy(d.data()));
                                }
                            }
                        },
                        PartitionOp::EltEdit(elt, editor) => {
                            let id: u64 = try!(elt.parse());
                            println!("TODO: operation not yet implemented");
                        },
                        PartitionOp::EltDelete(elt) => {
                            let id: u64 = try!(elt.parse());
                            try!(state.remove_elt(id));
                        },
                    }
                }       // destroy reference `state`
                
                let has_changes = try!(part.write(true));
                if has_changes && snapshot {
                    try!(part.write_snapshot());
                }
                Ok(())
            }
        },
        Operation::Default => {
            println!("Scanning files ...");
            
            //TODO: this should not assume all files belong to the same
            // partition, but discover this and use alternate behaviour in the
            // case of multiple partitions. Not that there's much point to this
            // default operation anyway.
            let discover = try!(DiscoverPartitionFiles::from_paths(paths));
            
            println!("Found {} snapshot file(s) and {} log file(s)",
                discover.num_ss_files(),
                discover.num_cl_files());
            Ok(())
        },
    }
//         } else if args.cmd_insert {
//             let id = extract(args.arg_id, "<id>");
//             let data = extract(args.arg_data, "<data>");
//             
//             match repo.insert_elt(id, Element::from_vec(data.into())) {
//                 Ok(()) => { println!("Element {} inserted.", id); },
//                 Err(e) => { println!("Element {} couldn't be inserted: {}", id, e); }
//             };
//             
//             //TODO: only if changed
//             try!(repo.save_file(&path));
//         }
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
