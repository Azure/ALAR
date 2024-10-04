#![no_main]

use libfuzzer_sys::fuzz_target;
use distro::distro::Distro;

fn harness(data: &[u8] {
    let data = Distro::new();
}

fuzz_target!(|data: &[u8]| {
    harness(data);
});
