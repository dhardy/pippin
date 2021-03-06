// Subject to the ISC licence (LICENSE-ISC.txt).

// The obligatory hello-world example.

use pippin::pip::{self, StateRead, StateWrite};

extern crate pippin;

// Main function — for error handling
fn inner() -> pip::Result<()> {
    // We try to find Pippin files in '.':
    println!("Looking for Pippin files in the current directory ...");
    match pip::part_from_path(".") {
        Ok(io) => {
            // Read the found files:
            let control = pip::DefaultControl::<String, _>::new(io);
            let part = pip::Partition::open(control, true)?;
            
            // Get access to the latest state:
            let tip = part.tip()?;
            println!("Found {} element(s)", tip.num_avail());
            
            // Read the elements (API may change here):
            for (id, elt) in tip.elts_iter() {
                println!("Element {}: {}", id, *elt);
            }
        },
        Err(e) => {
            println!("Error: {}", e);
            println!("Creating a new partition instead (run again to see contents)");
            
            // Create a new partition, using RepoFileIO:
            let io = pip::RepoFileIO::new("hello");
            let control = pip::DefaultControl::<String, _>::new(io);
            let mut part = pip::Partition::create(control, "hello world")?;
            
            // Create a new state derived from the tip:
            let mut state = part.tip()?.clone_mut();
            state.insert_new("Hello, world!".to_string())?;
            part.push_state(state)?;
            
            // Write our changes:
            part.write_full()?;
        }
    }
    Ok(())
}

fn main() {
    match inner() {
        Ok(()) => {},
        Err(e) => {
            println!("Error: {}", e);
        }
    }
}
