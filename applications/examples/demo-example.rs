extern crate pippin_app_tests;

use pippin_app_tests::util;

fn main() {
    println!("Demo example");
    let target_dir = util::get_target_dir();
    let top_dir = util::get_top_dir(&target_dir);
    println!("top: {}", top_dir.display());
    println!("target: {}", target_dir.display());
}
