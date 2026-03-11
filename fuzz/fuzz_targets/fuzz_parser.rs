#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Only test valid UTF-8 strings
    if let Ok(input) = std::str::from_utf8(data) {
        // The parser should never panic on any input
        let _ = kodo_parser::parse(input);
    }
});
