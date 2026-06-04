//! Fuzz target: feed arbitrary (font_data, gid_mask) into `subset_by_gids`.
//!
//! The GID set is derived from the first 8 bytes of input as a 64-bit bitmask
//! selecting GIDs 0..63.  GID 0 (.notdef) is always included per subset spec.
//!
//! Invariants verified:
//!   - `subset_by_gids` never panics on arbitrary input.
//!   - On success, the output is a valid SFNT (magic check).

#![no_main]

use libfuzzer_sys::fuzz_target;
use std::collections::BTreeSet;

fuzz_target!(|data: &[u8]| {
    if data.len() < 8 {
        return;
    }

    let mask = u64::from_le_bytes(data[..8].try_into().expect("8 bytes"));
    let mut gid_set: BTreeSet<u16> = (0u16..64)
        .filter(|&i| mask & (1u64 << i) != 0)
        .collect();
    // Always keep .notdef (GID 0).
    gid_set.insert(0);

    let font_data = &data[8.min(data.len())..];

    // subset_by_gids must never panic.
    let result = oxifont_subset::subset_by_gids(font_data, &gid_set);

    if let Ok(subset) = result {
        if subset.len() >= 4 {
            let magic = u32::from_be_bytes([subset[0], subset[1], subset[2], subset[3]]);
            let valid = matches!(
                magic,
                0x0001_0000 | 0x4F54_544F | 0x7472_7565 | 0x7479_7031 | 0x7474_6366
            );
            assert!(valid, "unexpected magic 0x{magic:08X}");
        }
    }
});
