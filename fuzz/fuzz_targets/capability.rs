#![no_main]

use std::str::FromStr;

use code::ShareCode;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    if let Ok(text) = std::str::from_utf8(input) {
        let _ = ShareCode::from_str(text);
    }
});
