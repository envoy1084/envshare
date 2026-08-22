#![no_main]

use libfuzzer_sys::fuzz_target;
use protocol::{decode_request_frame, decode_response_frame, parse_frame_length};

fuzz_target!(|frame: &[u8]| {
    let _ = parse_frame_length(frame);
    let _ = decode_request_frame(frame);
    let _ = decode_response_frame(frame);
});
