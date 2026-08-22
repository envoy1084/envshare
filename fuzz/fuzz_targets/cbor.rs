#![no_main]

use libfuzzer_sys::fuzz_target;
use protocol::{decode_request_frame, decode_response_frame};

fuzz_target!(|body: &[u8]| {
    let Ok(length) = u32::try_from(body.len()) else {
        return;
    };
    let mut frame = Vec::with_capacity(body.len() + 4);
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(body);
    let _ = decode_request_frame(&frame);
    let _ = decode_response_frame(&frame);
});
