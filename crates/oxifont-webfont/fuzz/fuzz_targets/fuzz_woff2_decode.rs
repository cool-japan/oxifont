//! Fuzz target: feed arbitrary bytes into `decode_woff2`.
//!
//! Invariants:
//!   - `decode_woff2` never panics on arbitrary input.
//!   - `decode_woff2_streaming` produces identical output to `decode_woff2`.

#![no_main]

use libfuzzer_sys::fuzz_target;
use std::io::Cursor;

fuzz_target!(|data: &[u8]| {
    // decode_woff2 must never panic.
    let result_slice = oxifont_webfont::decode_woff2(data);

    // decode_woff2_streaming must produce identical output.
    let result_streaming = oxifont_webfont::decode_woff2_streaming(Cursor::new(data));

    match (&result_slice, &result_streaming) {
        (Ok(sfnt_a), Ok(sfnt_b)) => {
            assert_eq!(
                sfnt_a, sfnt_b,
                "decode_woff2 and decode_woff2_streaming produced different output"
            );
        }
        (Err(_), Err(_)) => {
            // Both failed — consistent behaviour, acceptable.
        }
        _ => {
            // One succeeded and the other failed — this is a bug.
            panic!(
                "decode_woff2 vs streaming inconsistency: slice={:?}, streaming={:?}",
                result_slice.is_ok(),
                result_streaming.is_ok()
            );
        }
    }
});
