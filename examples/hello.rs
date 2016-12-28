// Subject to the ISC licence (LICENSE-ISC.txt).

// The obligatory hello-world example.

use pippin::{Result, Partition, StateT, MutStateT, discover, fileio};

extern crate pippin;

// Main function — for error handling
fn inner() -> Result<()> {
    // We try to find Pippin files in '.':
    println!("Looking for Pippin files in the current directory ...");
    match discover::part_from_path(".", None) {
        Ok(io) => {
            // Read the found files:
            let mut part = Partition::<String>::open(Box::new(io))?;
            part.load_latest(None, None)?;
            
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
            
            // Create a new partition, using PartFileIO:
            let io = Box::new(fileio::PartFileIO::new_default("hello"));
            let mut part = Partition::create(io, "hello world", None, None)?;
            
            // Create a new state derived from the tip:
            let mut state = part.tip()?.clone_mut();
            state.insert("Hello, world!".to_string())?;
            part.push_state(state, None)?;
            
            // Write our changes:
            part.write_full(None)?;
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
