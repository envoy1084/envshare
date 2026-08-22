#![no_main]

use crypto::Transcript;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &[u8]| {
    let split = input.len().min(32);
    if let Ok(mut transcript) = Transcript::new(b"envshare/fuzz/transcript/v1") {
        let _ = transcript.append_bytes(&input[..split]);
        let _ = transcript.append_bytes(&input[split..]);
        let _ = transcript.append_u64(input.len() as u64);
        let _ = transcript.finish();
    }
});
