//! Comprehensive systematic tests for `oxifont_core` types.
//!
//! These tests cover `FontQuery` builder semantics, `FontStyle` CSS matching
//! via `css_preference_score`, `FaceInfo` field access, and `FontError`
//! Display formatting.  No external test framework is used — the goal is
//! exhaustive coverage through carefully chosen representative cases rather
//! than generative fuzzing.

use oxifont_core::{FaceInfo, FontError, FontQuery, FontStretch, FontStyle};
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a minimal `FaceInfo` for use in tests.
fn make_face(family: &str, style: FontStyle, weight: u16) -> FaceInfo {
    FaceInfo {
        family: Arc::from(family),
        post_script_name: String::new(),
        style,
        weight,
        stretch: FontStretch::Normal,
        path: std::env::temp_dir().join("test.ttf"),
        face_index: 0,
        localized_families: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// FontQuery builder tests
// ---------------------------------------------------------------------------

#[test]
fn test_fontquery_default_values() {
    let q = FontQuery::new();
    assert!(q.family.is_none(), "default family should be None");
    assert!(q.style.is_none(), "default style should be None");
    assert!(q.weight.is_none(), "default weight should be None");
}

#[test]
fn test_fontquery_family_single() {
    let q = FontQuery::new().family("Helvetica");
    assert_eq!(q.family.as_deref(), Some("Helvetica"));
}

#[test]
fn test_fontquery_family_overrides() {
    // Calling `.family()` twice replaces the first value (no multi-family
    // chaining in the core query — that lives in `oxifont_db::Query`).
    let q = FontQuery::new().family("Arial").family("Helvetica");
    assert_eq!(q.family.as_deref(), Some("Helvetica"));
}

#[test]
fn test_fontquery_style_italic() {
    let q = FontQuery::new().style(FontStyle::Italic);
    assert_eq!(q.style, Some(FontStyle::Italic));
}

#[test]
fn test_fontquery_style_oblique() {
    let q = FontQuery::new().style(FontStyle::Oblique);
    assert_eq!(q.style, Some(FontStyle::Oblique));
}

#[test]
fn test_fontquery_style_normal() {
    let q = FontQuery::new().style(FontStyle::Normal);
    assert_eq!(q.style, Some(FontStyle::Normal));
}

#[test]
fn test_fontquery_weight_regular() {
    let q = FontQuery::new().weight(400);
    assert_eq!(q.weight, Some(400));
}

#[test]
fn test_fontquery_weight_thin() {
    let q = FontQuery::new().weight(100);
    assert_eq!(q.weight, Some(100));
}

#[test]
fn test_fontquery_weight_black() {
    let q = FontQuery::new().weight(900);
    assert_eq!(q.weight, Some(900));
}

#[test]
fn test_fontquery_weight_overrides() {
    let q = FontQuery::new().weight(400).weight(700);
    assert_eq!(q.weight, Some(700));
}

#[test]
fn test_fontquery_all_fields_combined() {
    let q = FontQuery::new()
        .family("Noto Sans")
        .style(FontStyle::Italic)
        .weight(600);
    assert_eq!(q.family.as_deref(), Some("Noto Sans"));
    assert_eq!(q.style, Some(FontStyle::Italic));
    assert_eq!(q.weight, Some(600));
}

#[test]
fn test_fontquery_clone_is_independent() {
    let q1 = FontQuery::new().family("Arial").weight(400);
    let mut q2 = q1.clone();
    q2.family = Some("Helvetica".to_string());
    // Original must not change.
    assert_eq!(q1.family.as_deref(), Some("Arial"));
}

// ---------------------------------------------------------------------------
// FontStyle equality
// ---------------------------------------------------------------------------

#[test]
fn test_fontstyle_eq_same_variants() {
    assert_eq!(FontStyle::Normal, FontStyle::Normal);
    assert_eq!(FontStyle::Italic, FontStyle::Italic);
    assert_eq!(FontStyle::Oblique, FontStyle::Oblique);
}

#[test]
fn test_fontstyle_ne_different_variants() {
    assert_ne!(FontStyle::Normal, FontStyle::Italic);
    assert_ne!(FontStyle::Italic, FontStyle::Oblique);
    assert_ne!(FontStyle::Normal, FontStyle::Oblique);
}

#[test]
fn test_fontstyle_default_is_normal() {
    let s: FontStyle = Default::default();
    assert_eq!(s, FontStyle::Normal);
}

// ---------------------------------------------------------------------------
// FontStyle::css_preference_score tests
// ---------------------------------------------------------------------------

/// When italic is requested, Italic is preferred over Oblique over Normal.
#[test]
fn test_css_preference_italic_requested() {
    let italic_vs_italic = FontStyle::css_preference_score(FontStyle::Italic, FontStyle::Italic);
    let italic_vs_oblique = FontStyle::css_preference_score(FontStyle::Italic, FontStyle::Oblique);
    let italic_vs_normal = FontStyle::css_preference_score(FontStyle::Italic, FontStyle::Normal);

    assert!(
        italic_vs_italic > italic_vs_oblique,
        "italic should score higher than oblique when italic requested"
    );
    assert!(
        italic_vs_oblique > italic_vs_normal,
        "oblique should score higher than normal when italic requested"
    );
}

/// When oblique is requested, Oblique is preferred over Italic over Normal.
#[test]
fn test_css_preference_oblique_requested() {
    let oblique_vs_oblique =
        FontStyle::css_preference_score(FontStyle::Oblique, FontStyle::Oblique);
    let oblique_vs_italic = FontStyle::css_preference_score(FontStyle::Oblique, FontStyle::Italic);
    let oblique_vs_normal = FontStyle::css_preference_score(FontStyle::Oblique, FontStyle::Normal);

    assert!(
        oblique_vs_oblique > oblique_vs_italic,
        "oblique should score higher than italic when oblique requested"
    );
    assert!(
        oblique_vs_italic > oblique_vs_normal,
        "italic should score higher than normal when oblique requested"
    );
}

/// When normal is requested, Normal is preferred over Oblique over Italic.
#[test]
fn test_css_preference_normal_requested() {
    let normal_vs_normal = FontStyle::css_preference_score(FontStyle::Normal, FontStyle::Normal);
    let normal_vs_oblique = FontStyle::css_preference_score(FontStyle::Normal, FontStyle::Oblique);
    let normal_vs_italic = FontStyle::css_preference_score(FontStyle::Normal, FontStyle::Italic);

    assert!(
        normal_vs_normal > normal_vs_oblique,
        "normal should score highest when normal requested"
    );
    assert!(
        normal_vs_oblique > normal_vs_italic,
        "oblique should score higher than italic when normal requested"
    );
}

/// A perfect match always returns the same score regardless of style.
#[test]
fn test_css_preference_exact_match_is_symmetric() {
    let s_italic = FontStyle::css_preference_score(FontStyle::Italic, FontStyle::Italic);
    let s_oblique = FontStyle::css_preference_score(FontStyle::Oblique, FontStyle::Oblique);
    let s_normal = FontStyle::css_preference_score(FontStyle::Normal, FontStyle::Normal);
    // All exact matches should return the same (maximum) score value.
    assert_eq!(s_italic, s_oblique);
    assert_eq!(s_oblique, s_normal);
}

/// An exact-match score is strictly greater than any non-match score for the
/// same requested style.
#[test]
fn test_css_preference_exact_match_beats_all_others() {
    for requested in [FontStyle::Italic, FontStyle::Oblique, FontStyle::Normal] {
        let exact = FontStyle::css_preference_score(requested.clone(), requested.clone());
        for available in [FontStyle::Italic, FontStyle::Oblique, FontStyle::Normal] {
            if available != requested {
                let other = FontStyle::css_preference_score(requested.clone(), available.clone());
                assert!(
                    exact > other,
                    "exact match {requested:?} should beat non-match available={available:?}"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// FaceInfo basic tests
// ---------------------------------------------------------------------------

#[test]
fn test_faceinfo_construction() {
    let face = make_face("Arial", FontStyle::Normal, 400);
    assert_eq!(&*face.family, "Arial");
    assert_eq!(face.style, FontStyle::Normal);
    assert_eq!(face.weight, 400);
    assert_eq!(face.face_index, 0);
}

#[test]
fn test_faceinfo_clone_is_independent() {
    let original = make_face("Helvetica", FontStyle::Italic, 700);
    let mut clone = original.clone();
    clone.family = Arc::from("Changed");
    assert_eq!(&*original.family, "Helvetica");
}

#[test]
fn test_faceinfo_all_styles() {
    for style in [FontStyle::Normal, FontStyle::Italic, FontStyle::Oblique] {
        let face = make_face("Test", style.clone(), 400);
        assert_eq!(face.style, style);
    }
}

#[test]
fn test_faceinfo_weight_range_boundaries() {
    // CSS allows weights 1..=1000; the common values are multiples of 100.
    for w in [100u16, 200, 300, 400, 500, 600, 700, 800, 900] {
        let face = make_face("Test", FontStyle::Normal, w);
        assert_eq!(face.weight, w);
    }
}

// ---------------------------------------------------------------------------
// FontError Display formatting
// ---------------------------------------------------------------------------

#[test]
fn test_fonterror_parse_error_display() {
    let e = FontError::ParseError("bad magic".to_string());
    let s = format!("{e}");
    assert!(
        s.contains("bad magic"),
        "display should include message: {s}"
    );
}

#[test]
fn test_fonterror_io_error_display() {
    let io = std::io::Error::new(std::io::ErrorKind::NotFound, "no such file");
    let e = FontError::IoError(io.into());
    let s = format!("{e}");
    assert!(
        s.contains("no such file"),
        "display should include io message: {s}"
    );
}

#[test]
fn test_fonterror_not_found_display() {
    let e = FontError::NotFound;
    let s = format!("{e}");
    assert!(
        s.contains("not found"),
        "display should mention not found: {s}"
    );
}

#[test]
fn test_fonterror_unsupported_format_display() {
    let e = FontError::UnsupportedFormat;
    let s = format!("{e}");
    assert!(
        s.contains("unsupported"),
        "display should mention unsupported: {s}"
    );
}

#[test]
fn test_fonterror_index_out_of_bounds_display() {
    let e = FontError::IndexOutOfBounds { index: 5, count: 3 };
    let s = format!("{e}");
    assert!(s.contains('5'), "display should include index: {s}");
    assert!(s.contains('3'), "display should include count: {s}");
}

#[test]
fn test_fonterror_from_io_error() {
    let io = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
    let e: FontError = io.into();
    // FontError::IoError wraps Arc<std::io::Error>
    assert!(matches!(e, FontError::IoError(_)));
}
