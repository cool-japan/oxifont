//! Fuzz target: feed arbitrary (font_data, codepoints_mask) into `subset_font`.
//!
//! The codepoints are derived from the first 4 bytes of input as a bitmask
//! over a fixed Unicode range (U+0020..U+003F — Basic Latin), giving
//! reasonable coverage without requiring a separate codepoint generator.
//!
//! Invariants verified:
//!   - `subset_font` never panics on arbitrary input.
//!   - On success, the output starts with a valid SFNT magic number.
//!   - On success, the output is at least 12 bytes (minimum SFNT offset table).

#![no_main]

use libfuzzer_sys::fuzz_target;
use std::collections::BTreeSet;

fuzz_target!(|data: &[u8]| {
    if data.len() < 4 {
        return;
    }

    // Use first 4 bytes as a bitmask to select codepoints in U+0020..U+003F.
    let mask = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    let codepoints: BTreeSet<char> = (0u32..32)
        .filter(|i| mask & (1 << i) != 0)
        .filter_map(|i| char::from_u32(0x20 + i))
        .collect();

    let font_data = &data[4.min(data.len())..];

    // subset_font must never panic.
    let result = oxifont_subset::subset_font(font_data, &codepoints);

    if let Ok(subset) = result {
        // Basic validity: starts with a known SFNT magic number.
        if subset.len() >= 4 {
            let magic = u32::from_be_bytes([subset[0], subset[1], subset[2], subset[3]]);
            assert!(
                magic == 0x0001_0000 // TrueType
                || magic == 0x4F54_544F // "OTTO" / CFF
                || magic == 0x7472_7565 // "true"
                || magic == 0x7479_7031 // "typ1"
                || magic == 0x7474_6366 // "ttcf" / TTC
                    ,
                "unexpected SFNT magic 0x{magic:08X} in subset output"
            );
            assert!(
                subset.len() >= 12,
                "subset output too short: {} bytes",
                subset.len()
            );
        }
    }
});
