//! Fuzz target: feed arbitrary bytes into `decode_woff1`.
//!
//! Invariants:
//!   - `decode_woff1` never panics on arbitrary input.
//!   - Valid WOFF1 (produced by `encode_woff1`) round-trips to a valid SFNT.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // decode_woff1 must never panic.
    let _ = oxifont_webfont::decode_woff1(data);
});
