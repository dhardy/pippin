//! Command-line UI for Pippin

extern crate pippin;
// extern crate rustc_serialize;
// extern crate docopt;
extern crate argparse;

use std::process::exit;
use std::path::Path;
use std::fs;
// use docopt::Docopt;
use argparse::{ArgumentParser, StoreTrue, StoreOption};
use pippin::{Repo, Result};

// Docopt stuff: doesn't actually work for me (arg_file etc. don't get set when passing -f <file> etc.).
// const USAGE: &'static str = "
// Pippin command-line UI.
// 
// Usage:
//   pippincmd stat [options]
//   pippincmd create [options]
//   pippincmd (-h | --help)
//   pippincmd --version
// 
// Options:
//   -f <file>     Repository file
//   -n <name>     Name for a new repository
//   -h --help     Show this message.
//   --version     Show version.
// ";

#[derive(Debug/*, RustcDecodable*/)]
struct Args {
    flag_version: bool,
    flag_force: bool,
    cmd_stat: bool,
    cmd_create: bool,
    arg_file: Option<String>,
    arg_repo_name: Option<String>,
    arg_elt: Option<u64>,
    arg_data: Option<String>,
}

fn main() {
//     let args: Args = Docopt::new(USAGE)
//                             .and_then(|d| d.decode())
//                             .unwrap_or_else(|e| e.exit());
    
    let mut args = Args { flag_version: false, flag_force: false,
        cmd_stat: false, cmd_create: false,
        arg_file: None, arg_repo_name: None,
        arg_elt: None, arg_data: None};
    
    { // scope limiting `ap`
        let mut ap = ArgumentParser::new();
        ap.set_description("Pippin command-line UI. This is a demo app and \
            should not be relied upon for any deployment. It is not optimised \
            for large repos and the UI may change.");
        ap.refer(&mut args.flag_version)
            .add_option(&["-v", "--version"], StoreTrue, "Show version");
        ap.refer(&mut args.flag_force)
            .add_option(&["-F", "--force"], StoreTrue, "Overwrite file/element instead of failing.");
        ap.refer(&mut args.cmd_stat)
            .add_option(&["-s", "--stat"], StoreTrue, "Show statistics for a repository");
        ap.refer(&mut args.cmd_create)
            .add_option(&["-c", "--create"], StoreTrue, "Create a new repository");
        ap.refer(&mut args.arg_file).metavar("<file>")
            .add_option(&["-f", "--file"], StoreOption, "Load/save repository from/to a file");
        ap.refer(&mut args.arg_repo_name).metavar("<repo_name>")
            .add_option(&["-n", "--name"], StoreOption, "Name for a repository");
        ap.refer(&mut args.arg_elt).metavar("<element_id>")
            .add_option(&["-e", "--element"], StoreOption, "Element identifier (numeric)");
        ap.refer(&mut args.arg_data).metavar("<data>")
            .add_option(&["-i", "--insert"], StoreOption, "Insert some element to the repo with this data");
        ap.parse_args_or_exit();
    }
    
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
    let load = args.cmd_stat || args.arg_data != None;
    if !load && !args.cmd_create {
        println!("Nothing to do (check usage).");
        return Ok(());
    }
    
    let path_str = extract(args.arg_file, "-f <file>");
    let path = Path::new(&path_str);
    //TODO: use path.is_file() when stable in libstd
    let is_file = if let Ok(m) = fs::metadata(path) {
        m.is_file() } else { false };
        
    if load {
        if !is_file {
            println!("This isn't a file or can't be opened: {}", path.display());
            exit(1);
        }
        let mut repo = try!(Repo::load_file(path));
        
        if args.cmd_stat {
            println!("Repo name: {}", repo.name());
            println!("Number of elements: {}", repo.num_elts());
        } else if let Some(data) = args.arg_data {
            let elt = extract(args.arg_elt, "-e <element_id>");
            
            repo.insert_elt(elt, data.as_bytes(), args.flag_force);
            
            try!(repo.save_file(path));
        }
    } else if args.cmd_create {
        let repo_name = extract(args.arg_repo_name, "-n <repo_name>");
        
        let repo = try!(Repo::new(repo_name));
        
        if !args.flag_force && is_file {
            println!("Already exists: {}", path.display());
            exit(1);
        }
        try!(repo.save_file(path));
    }
    Ok(())
}
