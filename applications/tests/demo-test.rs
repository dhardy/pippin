// Subject to the ISC licence (LICENSE-ISC.txt).

extern crate pippin_app_tests;

use pippin_app_tests::util;

#[test]
fn sample() {
    println!("Demo test");
    let target_dir = util::get_target_dir();
    let top_dir = util::get_top_dir(&target_dir);
    println!("top: {}", top_dir.display());
    println!("target: {}", target_dir.display());
}
