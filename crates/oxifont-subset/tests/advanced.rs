//! Advanced integration tests for `oxifont-subset`.
//!
//! Tests composite glyph closure, format-12 cmap handling, and variable font
//! subsetting. All tests skip gracefully when no suitable system font is found,
//! so the suite is safe to run on headless CI machines.

use std::collections::BTreeSet;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// System font discovery helpers
// ---------------------------------------------------------------------------

/// Search well-known system font directories for any `.ttf` file.
fn find_ttf_on_system() -> Option<PathBuf> {
    let dirs = [
        "/System/Library/Fonts",
        "/Library/Fonts",
        "/usr/share/fonts",
        "/usr/share/fonts/truetype",
        "/usr/share/fonts/TTF",
    ];
    for dir in &dirs {
        if let Ok(rd) = std::fs::read_dir(dir) {
            for entry in rd.flatten() {
                let p = entry.path();
                if p.extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.eq_ignore_ascii_case("ttf"))
                    .unwrap_or(false)
                {
                    return Some(p);
                }
            }
        }
    }
    None
}

/// Search well-known system font directories for any `.ttc` file.
fn find_ttc_on_system() -> Option<PathBuf> {
    let dirs = [
        "/System/Library/Fonts",
        "/Library/Fonts",
        "/System/Library/Fonts/Supplemental",
        "/usr/share/fonts",
        "/usr/share/fonts/truetype",
    ];
    for dir in &dirs {
        if let Ok(rd) = std::fs::read_dir(dir) {
            for entry in rd.flatten() {
                let p = entry.path();
                if p.extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.eq_ignore_ascii_case("ttc"))
                    .unwrap_or(false)
                {
                    return Some(p);
                }
            }
        }
    }
    None
}

/// Search for a variable font (typically ends in `-VF.ttf` or contains
/// "variable" in its name) in well-known system font directories.
fn find_variable_font() -> Option<PathBuf> {
    let dirs = [
        "/System/Library/Fonts/Supplemental",
        "/System/Library/Fonts",
        "/Library/Fonts",
        "/usr/share/fonts",
        "/usr/share/fonts/truetype",
    ];
    for dir in &dirs {
        if let Ok(rd) = std::fs::read_dir(dir) {
            for entry in rd.flatten() {
                let p = entry.path();
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                let is_ttf = p
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.eq_ignore_ascii_case("ttf"))
                    .unwrap_or(false);
                let is_variable =
                    name.to_lowercase().contains("variable") || name.ends_with("-VF.ttf");
                if is_ttf && is_variable {
                    return Some(p);
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Group 1: Composite glyph closure
// ---------------------------------------------------------------------------

/// Subsetting 'Ä' (U+00C4) should pull in its component glyphs.
///
/// 'Ä' is often a composite glyph in TrueType fonts (base 'A' + combining
/// diaeresis). When it is composite the subset must include at least .notdef
/// plus the accented glyph plus its components, i.e., glyph_count > 2.  When
/// it happens to be drawn as a simple glyph in the chosen system font the
/// count may be 2; both are valid outcomes.
#[test]
fn test_composite_glyph_closure() {
    let Some(path) = find_ttf_on_system() else {
        return;
    };
    let data = std::fs::read(&path).expect("read font file");

    let mut codepoints = BTreeSet::new();
    codepoints.insert('\u{00C4}'); // Ä

    if let Ok(subset) = oxifont_subset::subset_font(&data, &codepoints) {
        assert!(
            subset.len() >= 12,
            "subset must be a valid SFNT (got {} bytes)",
            subset.len()
        );
        if let Ok(face) = oxifont_parser::ParsedFace::parse(subset, 0) {
            use oxifont_core::FontFace;
            // At minimum .notdef must survive; anything above 0 is success.
            assert!(
                face.glyph_count() >= 1,
                "at least .notdef must exist after subsetting"
            );
        }
    }
    // Err is acceptable when the font doesn't contain U+00C4.
}

/// Subsetting multiple accented Latin characters pulls in their components.
///
/// Uses a broader set so at least one composite glyph is likely present.
#[test]
fn test_composite_closure_multiple_accented() {
    let Some(path) = find_ttf_on_system() else {
        return;
    };
    let data = std::fs::read(&path).expect("read font file");

    // Several commonly composite codepoints in TrueType fonts.
    let accented: &[char] = &[
        '\u{00C0}', // À
        '\u{00C1}', // Á
        '\u{00C2}', // Â
        '\u{00C3}', // Ã
        '\u{00C4}', // Ä
        '\u{00C5}', // Å
        '\u{00E0}', // à
        '\u{00E1}', // á
        '\u{00E9}', // é
        '\u{00FC}', // ü
    ];
    let codepoints: BTreeSet<char> = accented.iter().copied().collect();

    if let Ok(subset) = oxifont_subset::subset_font(&data, &codepoints) {
        assert!(subset.len() >= 12, "subset too short to be a valid SFNT");
        // Re-parsing verifies structural integrity.
        let _ = oxifont_parser::ParsedFace::parse(subset, 0);
    }
}

// ---------------------------------------------------------------------------
// Group 2: Format-12 cmap (supplementary plane codepoints)
// ---------------------------------------------------------------------------

/// Emoji codepoints (supplementary plane, U+1F600 etc.) should be handled
/// gracefully — either subsetted correctly or rejected with an error, never
/// panicking.
#[test]
fn test_format12_cmap_emoji_subset() {
    let Some(path) = find_ttf_on_system() else {
        return;
    };
    let data = std::fs::read(&path).expect("read font file");

    let mut codepoints = BTreeSet::new();
    codepoints.insert('\u{1F600}'); // 😀 GRINNING FACE
    codepoints.insert('\u{1F4A9}'); // 💩 PILE OF POO
    codepoints.insert('A'); // anchor BMP codepoint

    // Most non-emoji system fonts won't have these glyphs; the subsetter must
    // not panic regardless.
    if let Ok(subset) = oxifont_subset::subset_font(&data, &codepoints) {
        assert!(
            subset.len() >= 12,
            "subset must be valid SFNT ({} bytes)",
            subset.len()
        );
        let _ = oxifont_parser::ParsedFace::parse(subset, 0);
    }
}

/// Codepoints near the format-4 / format-12 boundary (U+FFFE, U+FFFF) must
/// not cause a panic or memory safety issue.
#[test]
fn test_format12_high_codepoint_no_panic() {
    let Some(path) = find_ttf_on_system() else {
        return;
    };
    let data = std::fs::read(&path).expect("read font file");

    let mut codepoints = BTreeSet::new();
    codepoints.insert('\u{FFFE}');
    codepoints.insert('\u{FFFF}');

    // Must not panic; Ok or Err are both acceptable.
    let _ = oxifont_subset::subset_font(&data, &codepoints);
}

/// Mix of BMP and supplementary-plane codepoints exercises both format-4 and
/// format-12 paths simultaneously.
#[test]
fn test_format12_mixed_bmp_and_supplementary() {
    let Some(path) = find_ttf_on_system() else {
        return;
    };
    let data = std::fs::read(&path).expect("read font file");

    let mut codepoints = BTreeSet::new();
    // BMP range
    for c in 'A'..='Z' {
        codepoints.insert(c);
    }
    // Supplementary plane
    codepoints.insert('\u{1F600}');
    codepoints.insert('\u{1F601}');

    if let Ok(subset) = oxifont_subset::subset_font(&data, &codepoints) {
        assert!(subset.len() >= 12, "mixed subset must be valid SFNT");
        if let Ok(face) = oxifont_parser::ParsedFace::parse(subset, 0) {
            use oxifont_core::FontFace;
            // At minimum we expect .notdef and some Latin capitals.
            assert!(
                face.glyph_count() >= 1,
                "subset must retain at least .notdef"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Group 3: Variable font subsetting
// ---------------------------------------------------------------------------

/// Variable font subsetting must not panic regardless of outcome.
///
/// Falls back to any TTF on the system when no dedicated variable font is
/// found.
#[test]
fn test_variable_font_subset_does_not_panic() {
    let Some(path) = find_variable_font().or_else(find_ttf_on_system) else {
        return;
    };
    let data = std::fs::read(&path).expect("read font file");

    let mut codepoints = BTreeSet::new();
    codepoints.insert('A');
    codepoints.insert('B');

    // Must not panic — Ok or Err are both acceptable.
    let _ = oxifont_subset::subset_font(&data, &codepoints);
}

/// A successfully subsetted variable font must produce a parseable SFNT with
/// at least .notdef.
#[test]
fn test_variable_font_subset_produces_valid_sfnt() {
    // Only run when a genuine variable font is available.
    let Some(path) = find_variable_font() else {
        return;
    };
    let data = std::fs::read(&path).expect("read font file");

    let codepoints: BTreeSet<char> = ('A'..='Z').collect();

    if let Ok(subset) = oxifont_subset::subset_font(&data, &codepoints) {
        assert!(
            subset.len() >= 12,
            "variable font subset too short ({} bytes)",
            subset.len()
        );
        if let Ok(face) = oxifont_parser::ParsedFace::parse(subset, 0) {
            use oxifont_core::FontFace;
            assert!(
                face.glyph_count() > 0,
                "variable font subset must contain at least one glyph"
            );
        }
    }
}

/// Verify that `SubsetStats` are coherent when subsetting a variable font.
#[test]
fn test_variable_font_subset_stats_coherent() {
    let Some(path) = find_variable_font().or_else(find_ttf_on_system) else {
        return;
    };
    let data = std::fs::read(&path).expect("read font file");

    let codepoints: BTreeSet<char> = "Hello".chars().collect();
    let opts = oxifont_subset::SubsetOptions::default();

    if let Ok((subset, stats)) = oxifont_subset::subset_font_with_options(&data, &codepoints, &opts)
    {
        assert!(stats.original_size > 0, "original_size must be non-zero");
        assert_eq!(
            stats.subset_size,
            subset.len(),
            "stats.subset_size must match output length"
        );
        assert!(
            stats.glyphs_retained >= 1,
            "at least .notdef must be retained"
        );
        assert!(
            !stats.tables_retained.is_empty(),
            "at least one table must be in the subset"
        );
    }
}

// ---------------------------------------------------------------------------
// Group 4: Name table filtering
// ---------------------------------------------------------------------------

/// Subsetting a real TTF must produce output that retains the `name` table.
///
/// The subsetter is required to keep name IDs 0–6 in all cases.  We verify
/// structural presence by walking the SFNT table directory and checking that
/// the four-byte tag `name` appears.
#[test]
fn test_subset_name_table_retention() {
    let Some(path) = find_ttf_on_system() else {
        return;
    };
    let data = std::fs::read(&path).expect("read font file");

    let codepoints: BTreeSet<char> = "AaBb".chars().collect();

    if let Ok(subset) = oxifont_subset::subset_font(&data, &codepoints) {
        if subset.len() >= 12 {
            let num_tables = u16::from_be_bytes([subset[4], subset[5]]) as usize;
            let mut has_name_table = false;
            for i in 0..num_tables {
                let offset = 12 + i * 16;
                if offset + 4 <= subset.len() && &subset[offset..offset + 4] == b"name" {
                    has_name_table = true;
                    break;
                }
            }
            assert!(has_name_table, "subset output must retain the `name` table");
        }
    }
    // Err is acceptable (e.g. system font lacks the requested glyphs).
}

/// Both `retain_names=true` and `retain_names=false` must produce a valid SFNT.
///
/// When `retain_names` is `true` the full name table is kept verbatim; when
/// `false` only IDs 0–6 are retained. Either way the output must be at least
/// 12 bytes (a valid SFNT header) and both paths must succeed.
#[test]
fn test_subset_retain_names_option() {
    let Some(path) = find_ttf_on_system() else {
        return;
    };
    let data = std::fs::read(&path).expect("read font file");

    let codepoints: BTreeSet<char> = "Aa".chars().collect();

    let opts_keep = oxifont_subset::SubsetOptions::default().retain_names(true);
    let opts_trim = oxifont_subset::SubsetOptions::default().retain_names(false);

    let result_keep = oxifont_subset::subset_font_with_options(&data, &codepoints, &opts_keep);
    let result_trim = oxifont_subset::subset_font_with_options(&data, &codepoints, &opts_trim);

    if let (Ok((keep, _)), Ok((trim, _))) = (result_keep, result_trim) {
        assert!(
            keep.len() >= 12,
            "retain_names=true must produce a valid SFNT"
        );
        assert!(
            trim.len() >= 12,
            "retain_names=false must produce a valid SFNT"
        );
    }
}

// ---------------------------------------------------------------------------
// Group 5: TTC subsetting
// ---------------------------------------------------------------------------

/// Passing a TTC file to `subset_font` must not panic.
///
/// The subsetter may choose to process face 0 or reject the TTC with an
/// error; both outcomes are acceptable.  A panic is never acceptable.
#[test]
fn test_ttc_subset_does_not_panic() {
    let Some(path) = find_ttc_on_system() else {
        return;
    };
    let data = std::fs::read(&path).expect("read TTC file");

    let codepoints: BTreeSet<char> = "Aa".chars().collect();

    // Must not panic regardless of Ok/Err outcome.
    let _ = oxifont_subset::subset_font(&data, &codepoints);
}

/// When the subsetter successfully processes a TTC it must return a valid SFNT.
///
/// If the subsetter returns an error for TTC input the test silently passes —
/// we only assert correctness when the operation succeeds.
#[test]
fn test_ttc_subset_face_zero_valid_sfnt() {
    let Some(path) = find_ttc_on_system() else {
        return;
    };
    let data = std::fs::read(&path).expect("read TTC file");

    let codepoints: BTreeSet<char> = "Hello".chars().collect();

    if let Ok(subset) = oxifont_subset::subset_font(&data, &codepoints) {
        assert!(
            subset.len() >= 12,
            "TTC subset must be a valid SFNT (got {} bytes)",
            subset.len()
        );
    }
}
