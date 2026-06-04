//! Fuzz target: feed arbitrary bytes into `detect_format` and `decode_auto`.
//!
//! Invariants:
//!   - `detect_format` never panics on arbitrary input.
//!   - `decode_auto` never panics on arbitrary input.
//!   - On short input (< 4 bytes), format is always `Unknown`.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    use oxifont_webfont::FontFormat;

    // detect_format must never panic.
    let fmt = oxifont_webfont::detect_format(data);

    // Short inputs must always return Unknown (no out-of-bounds read).
    if data.len() < 4 {
        assert!(
            matches!(fmt, FontFormat::Unknown),
            "short input returned non-Unknown format"
        );
    }

    // decode_auto must never panic.
    let _ = oxifont_webfont::decode_auto(data);
});
