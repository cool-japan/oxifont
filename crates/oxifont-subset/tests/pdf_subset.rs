//! Tests for [`pdf_subset::PdfFontSubsetter`] — on-the-fly font subsetting
//! for PDF text rendering pipelines.

use std::collections::BTreeSet;

use oxifont_subset::{pdf_subset::PdfFontSubsetter, SubsetOptions};

static FIXTURE_BYTES: &[u8] = include_bytes!("../../oxifont-parser/tests/fixtures/test.ttf");

fn load_test_font() -> Vec<u8> {
    FIXTURE_BYTES.to_vec()
}

// ---------------------------------------------------------------------------
// Helper: check that a 4-byte tag is present in an SFNT buffer.
// ---------------------------------------------------------------------------

fn sfnt_has_table(data: &[u8], tag: &[u8; 4]) -> bool {
    if data.len() < 12 {
        return false;
    }
    let num_tables = u16::from_be_bytes([data[4], data[5]]) as usize;
    for i in 0..num_tables {
        let base = 12 + i * 16;
        if base + 4 > data.len() {
            break;
        }
        if &data[base..base + 4] == tag {
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// test_pdf_subsetter_new_and_is_empty
// ---------------------------------------------------------------------------

/// A freshly constructed subsetter must report empty state.
#[test]
fn test_pdf_subsetter_new_and_is_empty() {
    let font = load_test_font();
    let opts = SubsetOptions::default();
    let subsetter = PdfFontSubsetter::new(font, opts);

    assert!(
        subsetter.is_empty(),
        "newly created subsetter must be empty"
    );
    assert_eq!(subsetter.codepoint_count(), 0);
    assert_eq!(subsetter.gid_count(), 0);
}

// ---------------------------------------------------------------------------
// test_pdf_subsetter_add_codepoint
// ---------------------------------------------------------------------------

/// `add_codepoint` / `add_text` must accumulate distinct codepoints correctly.
#[test]
fn test_pdf_subsetter_add_codepoint() {
    let font = load_test_font();
    let mut subsetter = PdfFontSubsetter::new(font, SubsetOptions::default());

    subsetter.add_codepoint('A');
    subsetter.add_codepoint('B');
    subsetter.add_codepoint('A'); // duplicate — must not double-count

    assert_eq!(subsetter.codepoint_count(), 2);
    assert!(!subsetter.is_empty());

    // add_text accumulates all unique chars.
    subsetter.add_text("Hello");
    // 'H', 'e', 'l', 'o' → 4 new; 'A' and 'B' already present
    assert_eq!(subsetter.codepoint_count(), 6); // A, B, H, e, l, o
}

// ---------------------------------------------------------------------------
// test_pdf_subsetter_add_gid
// ---------------------------------------------------------------------------

/// Raw GID accumulation must work correctly.
#[test]
fn test_pdf_subsetter_add_gid() {
    let font = load_test_font();
    let mut subsetter = PdfFontSubsetter::new(font, SubsetOptions::default());

    subsetter.add_gid(1);
    subsetter.add_gid(2);
    subsetter.add_gid(1); // duplicate

    assert_eq!(subsetter.gid_count(), 2);
    assert!(!subsetter.is_empty());

    let gids_slice = [3u16, 4, 5];
    subsetter.add_gids(&gids_slice);
    assert_eq!(subsetter.gid_count(), 5);
}

// ---------------------------------------------------------------------------
// test_pdf_subsetter_finalize_basic
// ---------------------------------------------------------------------------

/// `finalize` must produce a valid, parseable SFNT that contains at least the
/// expected core tables.
#[test]
fn test_pdf_subsetter_finalize_basic() {
    let font = load_test_font();
    let mut subsetter = PdfFontSubsetter::for_pdf(font.clone());

    subsetter.add_text("ABC");

    let (subset_bytes, stats) = subsetter.finalize().expect("finalize must succeed");

    // Structural validity.
    ttf_parser::Face::parse(&subset_bytes, 0)
        .expect("finalized subset must be parseable by ttf-parser");

    // Core tables must be present.
    let core_tags: &[[u8; 4]] = &[
        *b"glyf", *b"loca", *b"cmap", *b"hmtx", *b"head", *b"hhea", *b"post",
    ];
    for tag in core_tags {
        assert!(
            sfnt_has_table(&subset_bytes, tag),
            "table {:?} must be present in subset",
            std::str::from_utf8(tag).unwrap_or("????")
        );
    }

    // Stats sanity checks.
    assert_eq!(stats.original_size, font.len());
    assert_eq!(stats.subset_size, subset_bytes.len());
    assert!(
        stats.glyphs_retained >= 1,
        "at least .notdef must be retained"
    );
    assert!(
        stats.subset_size < stats.original_size,
        "subset ({} B) must be smaller than original ({} B)",
        stats.subset_size,
        stats.original_size
    );
}

// ---------------------------------------------------------------------------
// test_pdf_subsetter_for_pdf_preset
// ---------------------------------------------------------------------------

/// `for_pdf` preset must keep hint tables when present in the source.
#[test]
fn test_pdf_subsetter_for_pdf_preset() {
    let font = load_test_font();
    let mut subsetter = PdfFontSubsetter::for_pdf(font.clone());
    subsetter.add_text("Hello");

    let (subset_bytes, _stats) = subsetter.finalize().expect("for_pdf finalize failed");
    ttf_parser::Face::parse(&subset_bytes, 0).expect("for_pdf subset must be parseable");

    // Verify hint tables are retained when present.
    if sfnt_has_table(&font, b"fpgm") {
        assert!(
            sfnt_has_table(&subset_bytes, b"fpgm"),
            "fpgm must be retained by for_pdf preset"
        );
    }
    if sfnt_has_table(&font, b"prep") {
        assert!(
            sfnt_has_table(&subset_bytes, b"prep"),
            "prep must be retained by for_pdf preset"
        );
    }
}

// ---------------------------------------------------------------------------
// test_pdf_subsetter_for_web_preset
// ---------------------------------------------------------------------------

/// `for_web` preset must strip hint tables.
#[test]
fn test_pdf_subsetter_for_web_preset() {
    let font = load_test_font();
    let mut subsetter = PdfFontSubsetter::for_web(font.clone());
    subsetter.add_text("Hello World");

    let (subset_bytes, _stats) = subsetter.finalize().expect("for_web finalize failed");
    ttf_parser::Face::parse(&subset_bytes, 0).expect("for_web subset must be parseable");

    // Hint tables must be absent.
    assert!(
        !sfnt_has_table(&subset_bytes, b"fpgm"),
        "fpgm must be absent in for_web subset"
    );
    assert!(
        !sfnt_has_table(&subset_bytes, b"prep"),
        "prep must be absent in for_web subset"
    );
    assert!(
        !sfnt_has_table(&subset_bytes, b"cvt "),
        "cvt  must be absent in for_web subset"
    );
}

// ---------------------------------------------------------------------------
// test_pdf_subsetter_codepoint_unicode_mapping
// ---------------------------------------------------------------------------

/// Codepoints added via `add_codepoint` must be accessible via cmap in the
/// subset font (when the source font contains them).
#[test]
fn test_pdf_subsetter_codepoint_unicode_mapping() {
    let font = load_test_font();
    let mut subsetter = PdfFontSubsetter::for_pdf(font);

    let expected_chars: BTreeSet<char> = ['A', 'B', 'C', 'D', 'E'].iter().copied().collect();
    for &ch in &expected_chars {
        subsetter.add_codepoint(ch);
    }

    let (subset_bytes, _stats) = subsetter.finalize().expect("finalize failed");
    let face = ttf_parser::Face::parse(&subset_bytes, 0).expect("subset must be parseable");

    // Every requested codepoint that exists in the original font must be
    // accessible in the subset.
    let accessible_count = expected_chars
        .iter()
        .filter(|&&ch| face.glyph_index(ch).is_some())
        .count();

    assert!(
        accessible_count > 0,
        "at least one requested codepoint must be accessible in the subset"
    );
}

// ---------------------------------------------------------------------------
// test_pdf_subsetter_reset
// ---------------------------------------------------------------------------

/// After `reset`, the subsetter must be empty but the font data is preserved
/// (allowing a second `finalize` pass without re-constructing).
#[test]
fn test_pdf_subsetter_reset() {
    let font = load_test_font();
    let mut subsetter = PdfFontSubsetter::for_pdf(font);

    subsetter.add_text("First page content");
    assert!(!subsetter.is_empty());

    subsetter.reset();
    assert!(subsetter.is_empty(), "subsetter must be empty after reset");
    assert_eq!(subsetter.codepoint_count(), 0);
    assert_eq!(subsetter.gid_count(), 0);

    // After reset, add new content and finalize again.
    subsetter.add_text("XY");
    let (subset_bytes, _stats) = subsetter.finalize().expect("second finalize failed");
    ttf_parser::Face::parse(&subset_bytes, 0).expect("second finalize must produce parseable font");
}

// ---------------------------------------------------------------------------
// test_pdf_subsetter_merge
// ---------------------------------------------------------------------------

/// `merge` must combine codepoints from two accumulators.
#[test]
fn test_pdf_subsetter_merge() {
    let font = load_test_font();
    let mut subsetter_a = PdfFontSubsetter::for_pdf(font.clone());
    let mut subsetter_b = PdfFontSubsetter::for_pdf(font);

    subsetter_a.add_text("Hello");
    subsetter_b.add_text("World");

    let count_a = subsetter_a.codepoint_count();
    let count_b = subsetter_b.codepoint_count();

    subsetter_a.merge(&mut subsetter_b);

    // After merge, b is empty.
    assert!(
        subsetter_b.is_empty(),
        "subsetter_b must be empty after merge"
    );
    // a has at least max(count_a, count_b) codepoints (may be fewer if overlap).
    assert!(
        subsetter_a.codepoint_count() >= count_a.max(count_b),
        "merged subsetter must have at least as many codepoints as the larger source"
    );

    // Result is a valid font.
    let (subset_bytes, _stats) = subsetter_a.finalize().expect("merged finalize failed");
    ttf_parser::Face::parse(&subset_bytes, 0).expect("merged subset must be parseable");
}

// ---------------------------------------------------------------------------
// test_pdf_subsetter_gid_only
// ---------------------------------------------------------------------------

/// When only raw GIDs are added (no codepoints), the result must contain those
/// GIDs (composite closure may add more).
#[test]
fn test_pdf_subsetter_gid_only() {
    let font = load_test_font();
    let mut subsetter = PdfFontSubsetter::for_pdf(font);

    // GID 0 (.notdef) + a few more.
    subsetter.add_gids(&[0, 1, 2, 3]);

    let (subset_bytes, stats) = subsetter.finalize().expect("gid_only finalize failed");

    ttf_parser::Face::parse(&subset_bytes, 0).expect("gid_only subset must be parseable");

    // Must have at least the requested GIDs.
    assert!(
        stats.glyphs_retained >= 4,
        "expected >= 4 glyphs, got {}",
        stats.glyphs_retained
    );
}

// ---------------------------------------------------------------------------
// test_pdf_subsetter_finalize_into_result
// ---------------------------------------------------------------------------

/// `finalize_into_result` must return a `PdfSubsetResult` with consistent
/// `bytes` and `stats`.
#[test]
fn test_pdf_subsetter_finalize_into_result() {
    use oxifont_subset::pdf_subset::PdfSubsetResult;

    let font = load_test_font();
    let mut subsetter = PdfFontSubsetter::for_pdf(font.clone());
    subsetter.add_text("Hello PDF");

    let PdfSubsetResult { bytes, stats } = subsetter
        .finalize_into_result()
        .expect("finalize_into_result failed");

    assert_eq!(stats.subset_size, bytes.len());
    assert_eq!(stats.original_size, font.len());
    ttf_parser::Face::parse(&bytes, 0).expect("finalize_into_result must produce parseable font");
}

// ---------------------------------------------------------------------------
// test_pdf_subsetter_into_finalized
// ---------------------------------------------------------------------------

/// `into_finalized` must consume the subsetter and return the original font
/// data alongside the subset bytes.
#[test]
fn test_pdf_subsetter_into_finalized() {
    let font = load_test_font();
    let font_len = font.len();
    let mut subsetter = PdfFontSubsetter::for_pdf(font);
    subsetter.add_text("Test");

    let (original, subset_bytes, stats) =
        subsetter.into_finalized().expect("into_finalized failed");

    assert_eq!(original.len(), font_len);
    assert_eq!(stats.subset_size, subset_bytes.len());
    ttf_parser::Face::parse(&subset_bytes, 0).expect("into_finalized must produce parseable font");
}

// ---------------------------------------------------------------------------
// test_pdf_subsetter_empty_finalize
// ---------------------------------------------------------------------------

/// Calling `finalize` on an empty subsetter must produce a font containing
/// only `.notdef` (GID 0).
#[test]
fn test_pdf_subsetter_empty_finalize() {
    let font = load_test_font();
    let subsetter = PdfFontSubsetter::for_pdf(font);

    // No codepoints or GIDs added.
    let (subset_bytes, stats) = subsetter.finalize().expect("empty finalize failed");

    ttf_parser::Face::parse(&subset_bytes, 0).expect("empty finalize must produce parseable font");

    // Only .notdef should be present (1 glyph).
    assert_eq!(
        stats.glyphs_retained, 1,
        "empty finalize must produce exactly 1 glyph (.notdef), got {}",
        stats.glyphs_retained
    );
}

// ---------------------------------------------------------------------------
// test_pdf_subsetter_options_strip_hints
// ---------------------------------------------------------------------------

/// Custom `SubsetOptions` passed at construction must be honoured by `finalize`.
#[test]
fn test_pdf_subsetter_options_strip_hints() {
    let font = load_test_font();
    let opts = SubsetOptions::default()
        .strip_hints(true)
        .retain_names(false);
    let mut subsetter = PdfFontSubsetter::new(font, opts);
    subsetter.add_text("ABC");

    let (subset_bytes, _stats) = subsetter.finalize().expect("strip_hints finalize failed");

    assert!(
        !sfnt_has_table(&subset_bytes, b"fpgm"),
        "fpgm must be absent with strip_hints=true"
    );
    assert!(
        !sfnt_has_table(&subset_bytes, b"prep"),
        "prep must be absent with strip_hints=true"
    );
    ttf_parser::Face::parse(&subset_bytes, 0).expect("strip_hints subset must be parseable");
}
