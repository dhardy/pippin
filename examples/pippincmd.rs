/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Command-line UI for Pippin

extern crate pippin;
extern crate rustc_serialize;
extern crate docopt;
#[macro_use]
extern crate log;
extern crate env_logger;

use std::{fs, env, fmt, result};
use std::process::{exit, Command};
use std::path::PathBuf;
use std::io::{Read, Write};
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::rc::Rc;
use docopt::Docopt;
use pippin::{Partition, PartIO, ElementT, State, MutState, UserData, PartId};
use pippin::{discover, fileio};
use pippin::error::{Result, PathError, ErrorTrait};
use pippin::util::rtrim;
use pippin::readwrite::{read_head, FileType};

const USAGE: &'static str = "
Pippin command-line UI. This program is designed to demonstrate Pippin's
capabilities and to allow direct inspection of Pippin files. It is not intended
for automated usage and the UI may be subject to changes.

Usage:
  pippincmd [-h] -n PREFIX [-N NAME] [-i NUM] PATH
  pippincmd [-h] -H PATH
  pippincmd [-h] [-p NUM] [-P] [-S] [-L] [-C] PATH
  pippincmd [-h] [-f] [-p NUM] [-c COMMIT] [-s] [-E | -g ELT | -e ELT | -v ELT | -d ELT] PATH
  pippincmd --help | --version

Options:
  -n --new PREFIX       Create a new partition with file name prefix PREFIX.
                        A default state (no elements) is created.
                        If -N is not provided, PREFIX is used as the repo name.
  -N --repo-name NAME   Specify the name of the repository (stored in header).
  -i --part-num NUM     Specify the partition number (defaults to 1).
  -s --snapshot         Force writing of a snapshot after loading and applying
                        any changes, unless the state already has a snapshot.
  
  -H --header           Print out information from the file's header.
  
  -P --partitions       List all partitions loaded
  -p --partition NUM   Select partition NUM
  -S --snapshots        List all snapshots loaded
  -L --logs             List all log files loaded
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
  
  -f --force            Allow less common operations such as editing from a
                        historical state.
  
  -h --help             Show this message.
  --version             Show version.
";

#[derive(Debug, RustcDecodable)]
#[allow(non_snake_case)]        // names are mandated by docopt
struct Args {
    arg_PATH: Option<String>,
    flag_new: Option<String>,
    flag_repo_name: Option<String>,
    flag_part_num: Option<String>,
    flag_snapshot: bool,
    flag_header: bool,
    flag_partitions: bool,
    flag_partition: Option<String>,
    flag_snapshots: bool,
    flag_logs: bool,
    flag_commits: bool,
    flag_commit: Option<String>,
    flag_elements: bool,
    flag_get: Option<String>,
    flag_edit: Option<String>,
    flag_visual: Option<String>,
    flag_delete: Option<String>,
    flag_force: bool,
    flag_help: bool,
    flag_version: bool,
}

#[derive(Debug)]
enum PartitionOp {
    ListElts,
    EltGet(String),
    EltEdit(String, Editor),
    EltDelete(String),
}
#[derive(Debug)]
enum Editor { Cmd, Visual }
#[derive(Debug)]
enum Operation {
    NewPartition(String /*prefix*/, Option<String> /*repo name*/, Option<String> /* part num */),
    Header,
    List(bool /*list snapshot files?*/, bool /*list log files?*/, bool /*list commits?*/),
    OnPartition(PartitionOp),
}

fn main() {
    env_logger::init().unwrap();
    
    let args: Args = Docopt::new(USAGE)
                            .and_then(|dopt| dopt.decode())
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
                Operation::NewPartition(name, args.flag_repo_name, args.flag_part_num)
            } else if args.flag_header {
                Operation::Header
            } else if args.flag_partitions || args.flag_snapshots || args.flag_logs || args.flag_commits {
                Operation::List(args.flag_snapshots,
                        args.flag_logs, args.flag_commits)
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
                Operation::List(false, false, false)
            };
        let rest = Rest{ part: args.flag_partition, commit: args.flag_commit,
                snapshot: args.flag_snapshot, force: args.flag_force };
        let path = if let Some(path) = args.arg_PATH {
            PathBuf::from(path)
        } else {
            println!("A path is required (see --help)!");
            exit(1);
        };
        match inner(path, op, rest) {
            Ok(()) => {},
            Err(e) => {
                println!("Error: {}", e);
                exit(1);
            }
        }
    }
}

struct Rest {
    part: Option<String>,
    commit: Option<String>,
    snapshot: bool,
    force: bool,
}
fn inner(path: PathBuf, op: Operation, args: Rest) -> Result<()>
{
    match op {
        Operation::NewPartition(name, repo_name, part_num) => {
            assert_eq!(args.part, None);
            assert_eq!(args.commit, None);
            if !path.is_dir() {
                return PathError::err("Path to create new partition in must be a directory", path);
            }
            println!("Creating new partition: '{}' in {}", name, path.display());
            
            let repo_name = repo_name.unwrap_or_else(|| {
                let mut len = 16;
                while !name.is_char_boundary(len) { len -= 1; }
                name[0..len].to_string()
            });
            let part_id = PartId::from_num(match part_num {
                Some(n) => try!(n.parse()),
                None => 1,
            });
            
            let prefix = path.join(name);
            let io = fileio::PartFileIO::new_empty(part_id, prefix);
            try!(Partition::<DataElt>::create(Box::new(io), &repo_name,
                    vec![UserData::Text("by pippincmd".to_string())].into()));
            Ok(())
        },
        Operation::Header => {
            println!("Reading header from: {}", path.display());
            let head = try!(read_head(&mut try!(fs::File::open(path))));
            println!("{} file, version: {}",
                match head.ftype { FileType::Snapshot(_) => "Snapshot", FileType::CommitLog(_) => "Commit log" },
                head.ftype.ver());
            println!("Repository name: {}", head.name);
            print!("Partition number: ");
            match head.part_id {
                Some(id) => println!("{}", id.into_num()),
                None => println!("not specified"),
            };
            for ud in &*head.user {
                match ud {
                    &UserData::Data(ref d) =>
                        println!("User data (binary, length {})", d.len()),
                    &UserData::Text(ref t) =>
                        println!("User text: {}", t),
                };
            }
            Ok(())
        },
        Operation::List(list_snapshots, list_logs, list_commits) => {
            assert_eq!(args.commit, None);
            println!("Scanning files ...");
            // #0017: this should print warnings generated in discover::*
            let repo_files = try!(discover::repo_from_path(&path));
            for part in repo_files.partitions() {
                println!("Partition {}: {}*", part.part_id(), part.prefix().display());
                let ss_len = part.ss_len();
                if list_snapshots || list_logs {
                    for i in 0..ss_len {
                        if list_snapshots {
                            if let Some(p) = part.paths().get_ss(i) {
                                println!("Snapshot {:4}         : {}", i, p.display());
                            }
                        }
                        for j in 0..(if list_logs{ part.ss_cl_len(i) }else{0}) {
                            if let Some(p) = part.paths().get_cl(i, j) {
                                println!("Snapshot {:4} log {:4}: {}", i, j, p.display());
                            }
                        }
                    }
                } else {
                    if ss_len > 0 {
                        println!("Highest snapshot number: {}", ss_len - 1);
                    }
                }
                if list_commits {
                    let mut part = try!(Partition::<DataElt>::open(Box::new(part.clone())));
                    try!(part.load(true));
                    let mut states: Vec<_> = part.states().collect();
                    states.sort_by_key(|s| s.meta().number);
                    for state in states {
                        println!("Commit {:4}: {}; parents: {:?}",
                                state.meta().number, state.statesum(), 
                                state.parents());
                    }
                }
            }
            Ok(())
        },
        Operation::OnPartition(part_op) => {
            if args.part.is_some() {
                panic!("No support for -p / --partition option");
            }
            println!("Scanning files ...");
             let part_files = try!(discover::part_from_path(&path, None));
            
            let mut part = try!(Partition::<DataElt>::open(Box::new(part_files)));
            {
                let (is_tip, mut state) = if let Some(ss) = args.commit {
                    try!(part.load(true));
                    let state = try!(part.state_from_string(ss));
                    (part.tip_key().map(|k| k == state.statesum()).unwrap_or(false), state.clone_mut())
                } else {
                    try!(part.load(false));
                    (true, try!(part.tip()).clone_mut())
                };
                match part_op {
                    PartitionOp::ListElts => {
                        println!("Elements:");
                        for id in state.elt_map().keys() {
                            let n: u64 = (*id).into();
                            println!("  {}", n);
                        }
                    },
                    PartitionOp::EltGet(elt) => {
                        let id: u64 = try!(elt.parse());
                        match state.get(id.into()) {
                            Ok(d) => {
                                println!("Element {}:", id);
                                println!("{}", d);
                            },
                            Err(e) => { println!("Element {} {}", id, e.description()); },
                        }
                    },
                    PartitionOp::EltEdit(elt, editor) => {
                        if !is_tip && !args.force {
                            panic!("Do you really want to make an edit from a historical state? If so specify '--force'.");
                        }
                        let output = try!(Command::new("mktemp")
                            .arg("--tmpdir")
                            .arg("pippin-element.XXXXXXXX").output());
                        if !output.status.success() {
                            return CmdFailed::err("mktemp", output.status.code());
                        }
                        let tmp_path = PathBuf::from(OsStr::from_bytes(rtrim(&output.stdout, b'\n')));
                        if !tmp_path.is_file() {
                            return PathError::err("temporary file created but not found", tmp_path);
                        }
                        
                        let id: u64 = try!(elt.parse());
                        {
                            let elt_data: &DataElt = if let Ok(d) = state.get(id.into()) {
                                &d
                            } else {
                                panic!("element not found");
                            };
                            let mut file = try!(fs::OpenOptions::new().write(true).open(&tmp_path));
                            try!(file.write(elt_data.bytes()));
                        }
                        println!("Written to temporary file: {}", tmp_path.display());
                        
                        let editor_cmd = try!(env::var(match editor {
                            Editor::Cmd => "EDITOR",
                            Editor::Visual => "VISUAL",
                        }));
                        let status = try!(Command::new(&editor_cmd).arg(&tmp_path).status());
                        if !status.success() {
                            return CmdFailed::err(editor_cmd, status.code());
                        }
                        let mut file = try!(fs::File::open(&tmp_path));
                        let mut buf = Vec::new();
                        try!(file.read_to_end(&mut buf));
                        try!(state.replace(id.into(), DataElt::from(buf)));
                        try!(fs::remove_file(tmp_path));
                    },
                    PartitionOp::EltDelete(elt) => {
                        if !is_tip && !args.force {
                            panic!("Do you really want to make an edit from a historical state? If so specify '--force'.");
                        }
                        let id: u64 = try!(elt.parse());
                        try!(state.remove(id.into()));
                    },
                }
            }       // destroy reference `state`
            
            let user_fields: Rc<Vec<UserData>> = vec![UserData::Text("by pippincmd".to_string())].into();
            let has_changes = try!(part.write(true, user_fields.clone()));
            if has_changes && args.snapshot {
                try!(part.write_snapshot(user_fields));
            }
            Ok(())
        },
    }
}

#[derive(PartialEq, Debug)]
enum DataElt {
    Str(String),
    Bin(Vec<u8>),
}
impl DataElt {
    fn bytes(&self) -> &[u8] {
        match self {
            &DataElt::Str(ref s) => s.as_bytes(),
            &DataElt::Bin(ref v) => &v,
        }
    }
}
impl<'a> From<&'a [u8]> for DataElt {
    fn from(buf: &[u8]) -> DataElt {
        DataElt::from(Vec::from(buf))
    }
}
impl From<Vec<u8>> for DataElt {
    fn from(v: Vec<u8>) -> DataElt {
        match String::from_utf8(v) {
            Ok(s) => DataElt::Str(s),
            Err(e) => DataElt::Bin(e.into_bytes()),
        }
    }
}
impl ElementT for DataElt {
    fn write_buf(&self, writer: &mut Write) -> Result<()> {
        try!(writer.write(self.bytes()));
        Ok(())
    }
    fn read_buf(buf: &[u8]) -> Result<Self> {
        Ok(DataElt::from(buf))
    }
}
impl fmt::Display for DataElt {
    fn fmt(&self, f: &mut fmt::Formatter) -> result::Result<(), fmt::Error> {
        match self {
            &DataElt::Str(ref s) => write!(f, "String, {} bytes: {}", s.len(), s),
            &DataElt::Bin(ref v) => write!(f, "Binary, {} bytes: {}", String::from_utf8_lossy(v), v.len()),
        }
    }
}

/// Error type used to indicate a command failure
#[derive(Debug)]
pub struct CmdFailed {
    msg: String
}
impl CmdFailed {
    /// Create an "external command" error.
    pub fn err<S, T: fmt::Display>(cmd: T, status: Option<i32>) -> Result<S> {
        Err(Box::new(CmdFailed{ msg: match status {
            Some(code) => format!("external command failed with status {}: {}", code, cmd),
            None => format!("external command failed (interrupted): {}", cmd),
        }}))
    }
}
impl fmt::Display for CmdFailed {
    fn fmt(&self, f: &mut fmt::Formatter) -> result::Result<(), fmt::Error> {
        write!(f, "{}", self.msg)
    }
}
impl std::error::Error for CmdFailed {
    fn description(&self) -> &str { "tip not ready" }
}
