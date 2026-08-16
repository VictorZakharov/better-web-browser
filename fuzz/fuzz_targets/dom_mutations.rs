#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| better_web_browser::fuzzing::dom_mutations(data));
