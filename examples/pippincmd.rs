/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Command-line UI for Pippin

extern crate pippin;
extern crate rustc_serialize;
extern crate docopt;
extern crate log;
extern crate env_logger;

use std::{fs, env, fmt, result};
use std::process::{exit, Command};
use std::path::PathBuf;
use std::io::{Read, Write};
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::error::Error;

use docopt::Docopt;
use pippin::pip::*;
use pippin::rw::header::read_head;

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
    NewPartition(String /*prefix*/, Option<String> /*repo name*/),
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
                Operation::NewPartition(name, args.flag_repo_name)
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
        Operation::NewPartition(name, repo_name) => {
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
            
            let prefix = path.join(name);
            let io = RepoFileIO::new(prefix);
            let control = DefaultControl::<DataElt, _>::new(io);
            Partition::create(control, &repo_name)?;
            Ok(())
        },
        Operation::Header => {
            println!("Reading header from: {}", path.display());
            let head = read_head(&mut fs::File::open(path)?)?;
            println!("{} file, version: {}",
                match head.ftype { FileType::Snapshot(_) => "Snapshot", FileType::CommitLog(_) => "Commit log" },
                head.ftype.ver());
            println!("Repository name: {}", head.name);
            
            for ud in &*head.user {
                match *ud {
                    UserData::Data(ref d) =>
                        println!("User data (binary, length {})", d.len()),
                    UserData::Text(ref t) =>
                        println!("User text: {}", t),
                };
            }
            Ok(())
        },
        Operation::List(list_snapshots, list_logs, list_commits) => {
            assert_eq!(args.commit, None);
            println!("Scanning files ...");
            // #0017: this should print warnings generated in discover::*
            let part_files = part_from_path(&path)?;
            println!("Partition: {}*", part_files.prefix().display());
            let ss_len = part_files.ss_len();
            if list_snapshots || list_logs {
                for i in 0..ss_len {
                    if list_snapshots {
                        if let Some(p) = part_files.paths().get_ss(i) {
                            println!("Snapshot {:4}         : {}", i, p.display());
                        }
                    }
                    for j in 0..(if list_logs{ part_files.ss_cl_len(i) }else{0}) {
                        if let Some(p) = part_files.paths().get_cl(i, j) {
                            println!("Snapshot {:4} log {:4}: {}", i, j, p.display());
                        }
                    }
                }
            } else if ss_len > 0 {
                println!("Highest snapshot number: {}", ss_len - 1);
            }
            if list_commits {
                let control = DefaultControl::<DataElt, _>::new(part_files.clone());
                let mut part = Partition::open(control, true)?;
                part.load_all()?;
                let mut states: Vec<_> = part.states_iter().collect();
                states.sort_by_key(|s| s.meta().number());
                for state in states {
                    println!("Commit {:4}: {}; parents: {:?}",
                            state.meta().number(), state.statesum(), 
                            state.parents());
                }
            }
            Ok(())
        },
        Operation::OnPartition(part_op) => {
            if args.part.is_some() {
                panic!("No support for -p / --partition option");
            }
            println!("Scanning files ...");
            let part_files = part_from_path(&path)?;
            
            let control = DefaultControl::new(part_files);
            let mut part = Partition::open(control, true)?;
            {
                let (is_tip, mut state) = if let Some(ss) = args.commit {
                    part.load_all()?;
                    let state = part.state_from_string(ss)?;
                    (part.tip_key().map(|k| k == state.statesum()).unwrap_or(false), state.clone_mut())
                } else {
                    (true, part.tip()?.clone_mut())
                };
                match part_op {
                    PartitionOp::ListElts => {
                        println!("Elements:");
                        for (id,_) in state.elts_iter() {
                            let n: u64 = id.into();
                            println!("  {}", n);
                        }
                    },
                    PartitionOp::EltGet(elt) => {
                        let id: u64 = elt.parse()?;
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
                        let output = Command::new("mktemp")
                            .arg("--tmpdir")
                            .arg("pippin-element.XXXXXXXX").output()?;
                        if !output.status.success() {
                            return CmdFailed::err("mktemp", output.status.code());
                        }
                        let tmp_path = PathBuf::from(OsStr::from_bytes(rtrim(&output.stdout, b'\n')));
                        if !tmp_path.is_file() {
                            return PathError::err("temporary file created but not found", tmp_path);
                        }
                        
                        let id: u64 = elt.parse()?;
                        {
                            let elt_data: &DataElt = if let Ok(d) = state.get(id.into()) {
                                d
                            } else {
                                panic!("element not found");
                            };
                            let mut file = fs::OpenOptions::new().write(true).open(&tmp_path)?;
                            file.write_all(elt_data.bytes())?;
                        }
                        println!("Written to temporary file: {}", tmp_path.display());
                        
                        let editor_cmd = env::var(match editor {
                            Editor::Cmd => "EDITOR",
                            Editor::Visual => "VISUAL",
                        })?;
                        let status = Command::new(&editor_cmd).arg(&tmp_path).status()?;
                        if !status.success() {
                            return CmdFailed::err(editor_cmd, status.code());
                        }
                        let mut file = fs::File::open(&tmp_path)?;
                        let mut buf = Vec::new();
                        file.read_to_end(&mut buf)?;
                        state.replace(id.into(), DataElt::from(buf))?;
                        fs::remove_file(tmp_path)?;
                    },
                    PartitionOp::EltDelete(elt) => {
                        if !is_tip && !args.force {
                            panic!("Do you really want to make an edit from a historical state? If so specify '--force'.");
                        }
                        let id: u64 = elt.parse()?;
                        state.remove(id.into())?;
                    },
                }
            }       // destroy reference `state`
            
            let has_changes = part.write_fast()?;
            if has_changes && args.snapshot {
                part.write_snapshot()?;
            }
            Ok(())
        },
    }
}

#[derive(PartialEq, Eq, Debug)]
enum DataElt {
    Str(String),
    Bin(Vec<u8>),
}
impl DataElt {
    fn bytes(&self) -> &[u8] {
        match *self {
            DataElt::Str(ref s) => s.as_bytes(),
            DataElt::Bin(ref v) => v,
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
impl Element for DataElt {
    fn write_buf(&self, writer: &mut Write) -> Result<()> {
        writer.write_all(self.bytes())?;
        Ok(())
    }
    fn read_buf(buf: &[u8]) -> Result<Self> {
        Ok(DataElt::from(buf))
    }
}
impl fmt::Display for DataElt {
    fn fmt(&self, f: &mut fmt::Formatter) -> result::Result<(), fmt::Error> {
        match *self {
            DataElt::Str(ref s) => write!(f, "String, {} bytes: {}", s.len(), s),
            DataElt::Bin(ref v) => write!(f, "Binary, {} bytes: {}", String::from_utf8_lossy(v), v.len()),
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
