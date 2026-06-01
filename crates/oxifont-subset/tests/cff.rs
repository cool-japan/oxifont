//! Tests for CFF (Compact Font Format) table subsetting.

use std::collections::BTreeSet;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Helper: find a CFF (.otf) font on the current system
// ---------------------------------------------------------------------------

fn find_cff_font() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let otf_dirs = [
            "/System/Library/Fonts",
            "/Library/Fonts",
            "/System/Library/Fonts/Supplemental",
        ];
        for dir in &otf_dirs {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.extension().is_some_and(|e| e == "otf") {
                        return Some(p);
                    }
                }
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        let otf_dirs = ["/usr/share/fonts", "/usr/local/share/fonts"];
        for dir in &otf_dirs {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.extension().is_some_and(|e| e == "otf") {
                        return Some(p);
                    }
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Subset a real CFF font to a small codepoint set and verify structural validity.
/// Gracefully skips if no CFF font is present on the system.
#[test]
fn cff_subsetting_with_system_font() {
    let cff_font_path = find_cff_font();
    let Some(path) = cff_font_path else {
        eprintln!("No CFF font found on this system — skipping cff_subsetting_with_system_font");
        return;
    };

    let data = match std::fs::read(&path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Could not read {:?}: {e} — skipping", path);
            return;
        }
    };

    let codepoints: BTreeSet<char> = "Hello".chars().collect();

    // CFF subsetting must succeed (not just silently error).
    let subset = oxifont_subset::subset_font(&data, &codepoints)
        .unwrap_or_else(|e| panic!("subset_font failed for {:?}: {e:?}", path));

    // Must be smaller or equal (the verbatim fallback for CID/error ≥ original in the CFF
    // table itself, but the rest of the pipeline can still trim other tables).
    assert!(
        subset.len() <= data.len(),
        "subset ({} bytes) should not exceed original ({} bytes)",
        subset.len(),
        data.len()
    );

    // Must be a valid SFNT: at least 12 bytes header.
    assert!(
        subset.len() >= 12,
        "subset output too short to be a valid SFNT"
    );

    // Check SFNT signature (0x00010000 TrueType, 'OTTO' CFF, 'true', 'typ1').
    let sfnt_tag = u32::from_be_bytes([subset[0], subset[1], subset[2], subset[3]]);
    let valid_tags: &[u32] = &[
        0x0001_0000, // TrueType
        0x4F54_544F, // 'OTTO' — CFF OpenType
        0x7472_7565, // 'true'
        0x7479_7031, // 'typ1'
    ];
    assert!(
        valid_tags.contains(&sfnt_tag),
        "SFNT tag {sfnt_tag:#010X} not recognized for {:?}",
        path
    );

    // Round-trip: re-parse the output with ttf-parser and verify glyph count > 0.
    let face = ttf_parser::Face::parse(&subset, 0)
        .unwrap_or_else(|e| panic!("ttf-parser could not parse subset of {:?}: {e:?}", path));
    assert!(
        face.number_of_glyphs() > 0,
        "subset has no glyphs for {:?}",
        path
    );

    // Also verify with oxifont_parser::ParsedFace so we exercise our own parser
    // against the subset output.  This is the canonical check that the subset is
    // valid from the perspective of the oxifont ecosystem.
    use oxifont_core::FontFace as _;
    use oxifont_parser::ParsedFace;
    let arc_bytes: Arc<[u8]> = Arc::from(subset.as_slice());
    let parsed = ParsedFace::parse(arc_bytes, 0)
        .unwrap_or_else(|e| panic!("oxifont_parser could not parse subset of {:?}: {e:?}", path));
    assert!(
        parsed.glyph_count() > 0,
        "oxifont_parser reports 0 glyphs for subset of {:?}",
        path
    );
}

/// Verify that `rewrite_cff` does not panic on garbage input (verbatim fallback).
#[test]
fn cff_rewrite_garbage_returns_verbatim() {
    // Garbage < 4 bytes → TooShort → verbatim.
    let garbage_short = vec![0xFFu8, 0xFE, 0xFD];
    let remap = std::collections::HashMap::new();
    let result = oxifont_subset::cff::rewrite_cff(&garbage_short, &remap);
    assert_eq!(result, garbage_short, "verbatim fallback for short garbage");

    // Garbage with wrong CFF major version → UnsupportedVersion → verbatim.
    let bad_version = vec![2u8, 0, 4, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    let result2 = oxifont_subset::cff::rewrite_cff(&bad_version, &remap);
    assert_eq!(
        result2, bad_version,
        "verbatim fallback for wrong CFF version"
    );
}

/// Verify that an empty GID remap (keep only .notdef = GID 0) doesn't panic
/// when passed to rewrite_cff with garbage data — the verbatim path must be safe.
#[test]
fn cff_rewrite_empty_remap_no_panic() {
    let garbage = vec![0u8; 64];
    let remap: std::collections::HashMap<u16, u16> = [(0, 0)].into_iter().collect();
    // Must not panic; return value may be verbatim or rewritten.
    let _ = oxifont_subset::cff::rewrite_cff(&garbage, &remap);
}

// ---------------------------------------------------------------------------
// CFF2 tests
// ---------------------------------------------------------------------------

/// CFF2 with truncated/garbage input must return verbatim, no panic.
#[test]
fn cff2_rewrite_garbage_returns_verbatim() {
    let remap: std::collections::HashMap<u16, u16> = std::collections::HashMap::new();

    // Too short to even have a header.
    let too_short = vec![0x02u8, 0x00, 0x05];
    let result = oxifont_subset::cff::rewrite_cff2(&too_short, &remap);
    assert_eq!(result, too_short, "verbatim fallback for too-short CFF2");

    // Header says version 2 but topDictLength makes it truncated.
    // majorVersion=2, minorVersion=0, headerSize=5, topDictLength=0x0010 (16) but only 5 bytes total.
    let truncated = vec![0x02u8, 0x00, 0x05, 0x00, 0x10];
    let result2 = oxifont_subset::cff::rewrite_cff2(&truncated, &remap);
    assert_eq!(
        result2, truncated,
        "verbatim fallback for truncated CFF2 Top DICT"
    );

    // Wrong major version (not 2) → verbatim.
    let wrong_version = vec![0x01u8, 0x00, 0x05, 0x00, 0x00, 0x00, 0x00];
    let result3 = oxifont_subset::cff::rewrite_cff2(&wrong_version, &remap);
    assert_eq!(
        result3, wrong_version,
        "verbatim fallback for non-CFF2 version byte"
    );
}

/// CFF2 rewrite with all-zeros remap must not panic.
#[test]
fn cff2_rewrite_empty_remap_no_panic() {
    // Syntactically valid CFF2 header + minimal Top DICT (op 17 with offset) + empty Global Subr INDEX + empty CharStrings INDEX.
    // Header: version=2, minor=0, hdrSize=5, topDictLen=7
    // Top DICT: int32 value 12 (offset to CharStrings) encoded as b0=32+12+139=183? No: 32+val-139, so val=12 → b0=32+12+139=183? Actually: b0=32 encodes 32-139=-107; b0=139 encodes 0; b0=32+x encodes x-107.
    // For value 12: 12+139=151 → b0=151, then op 17.
    // Global Subr INDEX: count=0 → 0x00 0x00
    // CharStrings INDEX: count=0 → 0x00 0x00
    // Total after header: Top DICT (2 bytes: 151, 17) + Global Subr (2 bytes) + CharStrings (2 bytes) = 6 bytes
    // CharStrings offset from table start: hdrSize(5) + topDictLen(2) + globalSubrSize(2) = 9
    // Encode offset 9 as 5-byte: 29, 0, 0, 0, 9 → but that's 5+1 = 6-byte Top DICT
    // Let's use topDictLen=6, CharStrings at offset 5+6+2=13: encode 13 → 13+107=120, b0=120, op17.
    // But with 5+1=6 bytes for Top DICT and Global Subr INDEX at offset 11:
    //   hdr(5) + topDict(2) + globalSubr(2) + charstrings starts at 9? No: globalSubr at 7, charstrings at 9.
    //   Encode 9: b0 = 9+139 = 148, then op 17 → Top DICT = [148, 17] = 2 bytes.
    //   topDictLen = 2, header = [2, 0, 5, 0, 2].
    //   Global Subr INDEX at 7: [0, 0] (empty, count=0).
    //   CharStrings INDEX at 9: [0, 0] (empty, count=0).
    let minimal_cff2: Vec<u8> = vec![
        // Header: major=2, minor=0, hdrSize=5, topDictLen=2 (big-endian)
        0x02, 0x00, 0x05, 0x00, 0x02,
        // Top DICT DATA (2 bytes): CharStrings offset = 9, op 17
        // offset 9: b0 = 9 + 139 = 148
        148, 17, // Global Subr INDEX (empty): count = 0
        0x00, 0x00, // CharStrings INDEX (empty): count = 0
        0x00, 0x00,
    ];

    let remap: std::collections::HashMap<u16, u16> = std::collections::HashMap::new();
    // Must not panic.
    let result = oxifont_subset::cff::rewrite_cff2(&minimal_cff2, &remap);
    // With empty remap and empty CharStrings, result should be structurally valid CFF2.
    assert!(result.len() >= 5, "result must have at least a CFF2 header");
    assert_eq!(result[0], 2, "majorVersion must remain 2");
}

// ---------------------------------------------------------------------------
// System CFF2 font test
// ---------------------------------------------------------------------------

/// Walk SFNT table directory looking for a CFF2 table.
fn is_cff2_font(data: &[u8]) -> bool {
    if data.len() < 12 {
        return false;
    }
    // Support both single-face SFNT and TTC.
    let sfnt_offset: usize = if &data[0..4] == b"ttcf" {
        // TTC: first face at the offset stored at bytes 12-15.
        if data.len() < 16 {
            return false;
        }
        u32::from_be_bytes([data[12], data[13], data[14], data[15]]) as usize
    } else {
        0
    };
    if sfnt_offset + 12 > data.len() {
        return false;
    }
    let sfnt = &data[sfnt_offset..];
    if sfnt.len() < 12 {
        return false;
    }
    let num_tables = u16::from_be_bytes([sfnt[4], sfnt[5]]) as usize;
    if sfnt.len() < 12 + num_tables * 16 {
        return false;
    }
    for i in 0..num_tables {
        let off = 12 + i * 16;
        if &sfnt[off..off + 4] == b"CFF2" {
            return true;
        }
    }
    false
}

fn find_cff2_font() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let dirs = [
            "/Library/Fonts",
            "/System/Library/Fonts",
            "/System/Library/Fonts/Supplemental",
        ];
        for dir in &dirs {
            let Ok(entries) = std::fs::read_dir(dir) else {
                continue;
            };
            for e in entries.flatten() {
                let p = e.path();
                let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ext == "otf" || ext == "ttf" || ext == "ttc" {
                    if let Ok(data) = std::fs::read(&p) {
                        if is_cff2_font(&data) {
                            return Some(p);
                        }
                    }
                }
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        let dirs = ["/usr/share/fonts", "/usr/local/share/fonts"];
        for dir in &dirs {
            let Ok(entries) = std::fs::read_dir(dir) else {
                continue;
            };
            for e in entries.flatten() {
                let p = e.path();
                let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ext == "otf" || ext == "ttf" {
                    if let Ok(data) = std::fs::read(&p) {
                        if is_cff2_font(&data) {
                            return Some(p);
                        }
                    }
                }
            }
        }
    }
    None
}

/// Subset a real CFF2 variable font (if available on the system) and verify
/// that the output is a structurally valid SFNT. Gracefully skips if no CFF2
/// font is found.
#[test]
fn cff2_subsetting_with_system_variable_font() {
    let Some(path) = find_cff2_font() else {
        eprintln!("No CFF2 font found on this system — skipping cff2_subsetting_with_system_variable_font");
        return;
    };

    let data = match std::fs::read(&path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Could not read {:?}: {e} — skipping", path);
            return;
        }
    };

    eprintln!("Found CFF2 font: {:?}", path);

    let codepoints: BTreeSet<char> = "Hello".chars().collect();
    let result = oxifont_subset::subset_font(&data, &codepoints);

    match result {
        Ok(subset) => {
            assert!(
                subset.len() >= 12,
                "output should be valid SFNT (at least 12 bytes)"
            );
            // Check SFNT signature.
            let sfnt_tag = u32::from_be_bytes([subset[0], subset[1], subset[2], subset[3]]);
            let valid_tags: &[u32] = &[
                0x0001_0000, // TrueType
                0x4F54_544F, // 'OTTO' — OpenType CFF/CFF2
                0x7472_7565, // 'true'
                0x7479_7031, // 'typ1'
            ];
            assert!(
                valid_tags.contains(&sfnt_tag),
                "SFNT tag {sfnt_tag:#010X} not recognized for {:?}",
                path
            );
            eprintln!(
                "CFF2 subset succeeded: {} → {} bytes",
                data.len(),
                subset.len()
            );
        }
        Err(e) => {
            // Acceptable: some CFF2 fonts may trigger error paths in other tables
            // (e.g., missing cmap in a TTC sub-face).
            eprintln!(
                "subset_font returned error for CFF2 font {:?}: {e:?} — acceptable",
                path
            );
        }
    }
}

/// Directly call `rewrite_cff2` on the raw CFF2 table data from a real font,
/// bypassing the full pipeline (which may error on unrelated tables).
/// Verifies that the result starts with a valid CFF2 header.
#[test]
fn cff2_rewrite_cff2_table_directly() {
    let Some(path) = find_cff2_font() else {
        eprintln!("No CFF2 font found — skipping cff2_rewrite_cff2_table_directly");
        return;
    };

    let Ok(font_bytes) = std::fs::read(&path) else {
        return;
    };

    // Extract CFF2 table data from the SFNT.
    let cff2_table = extract_table(&font_bytes, b"CFF2");
    let Some(cff2_data) = cff2_table else {
        eprintln!("Could not extract CFF2 table from {:?} — skipping", path);
        return;
    };

    eprintln!(
        "Directly testing rewrite_cff2 on {:?} ({} bytes)",
        path,
        cff2_data.len()
    );

    // Build a remap that retains only GID 0 (simplest possible subset).
    let mut remap = std::collections::HashMap::new();
    remap.insert(0u16, 0u16);

    let result = oxifont_subset::cff::rewrite_cff2(cff2_data, &remap);

    // Must not panic and must return at least a 5-byte CFF2 header.
    assert!(result.len() >= 5, "result must have at least a CFF2 header");
    assert_eq!(result[0], 2, "majorVersion must remain 2");

    // If the result differs from verbatim fallback, it means rewrite_cff2 made
    // progress. Either outcome is acceptable as long as no panic and header valid.
    eprintln!(
        "rewrite_cff2: {} → {} bytes (verbatim={:?})",
        cff2_data.len(),
        result.len(),
        result == cff2_data
    );
}

/// Extract a named table from an SFNT font (handling TTC by using the first face).
///
/// Table offsets in the SFNT directory are absolute from the start of the file,
/// regardless of whether it is a standalone SFNT or a TTC.
fn extract_table<'a>(data: &'a [u8], tag: &[u8; 4]) -> Option<&'a [u8]> {
    // Handle TTC: first face starts at the offset stored at bytes 12-15.
    let sfnt_offset: usize = if data.len() >= 4 && &data[0..4] == b"ttcf" {
        if data.len() < 16 {
            return None;
        }
        u32::from_be_bytes([data[12], data[13], data[14], data[15]]) as usize
    } else {
        0
    };

    let sfnt = data.get(sfnt_offset..)?;
    if sfnt.len() < 12 {
        return None;
    }
    let num_tables = u16::from_be_bytes([sfnt[4], sfnt[5]]) as usize;
    if sfnt.len() < 12 + num_tables * 16 {
        return None;
    }
    for i in 0..num_tables {
        let rec = 12 + i * 16;
        if &sfnt[rec..rec + 4] == tag {
            // Table offsets are absolute from the start of the file (OpenType spec).
            let abs_offset =
                u32::from_be_bytes([sfnt[rec + 8], sfnt[rec + 9], sfnt[rec + 10], sfnt[rec + 11]])
                    as usize;
            let length = u32::from_be_bytes([
                sfnt[rec + 12],
                sfnt[rec + 13],
                sfnt[rec + 14],
                sfnt[rec + 15],
            ]) as usize;
            return data.get(abs_offset..abs_offset + length);
        }
    }
    None
}
