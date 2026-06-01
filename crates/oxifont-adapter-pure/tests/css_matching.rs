//! Integration tests for CSS Fonts Level 4 matching in `FontDatabase`.

use oxifont_adapter_pure::FontDatabase;
use oxifont_core::{FaceInfo, FontQuery, FontStretch, FontStyle};
use std::path::PathBuf;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Helper: construct a FaceInfo with minimal required fields
// ---------------------------------------------------------------------------

fn make_face(family: &str, style: FontStyle, weight: u16, stretch: FontStretch) -> FaceInfo {
    FaceInfo {
        family: Arc::from(family),
        post_script_name: String::new(),
        style,
        weight,
        stretch,
        path: PathBuf::from("/dev/null"),
        face_index: 0,
        localized_families: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// test_find_css_by_family
// ---------------------------------------------------------------------------

/// A database with "Arial" and "Helvetica" faces; querying for "Arial" returns
/// the Arial face (exact case-insensitive match).
#[test]
fn test_find_css_by_family() {
    let db = FontDatabase::from_faces(vec![
        make_face("Arial", FontStyle::Normal, 400, FontStretch::Normal),
        make_face("Helvetica", FontStyle::Normal, 400, FontStretch::Normal),
    ]);

    let result = db.find_css(&FontQuery::new().family("Arial"));
    assert!(result.is_some(), "Arial must be found");
    assert_eq!(&*result.unwrap().family, "Arial");
}

/// Same, but querying with mixed-case should still match.
#[test]
fn test_find_css_by_family_case_insensitive() {
    let db = FontDatabase::from_faces(vec![make_face(
        "Arial",
        FontStyle::Normal,
        400,
        FontStretch::Normal,
    )]);

    let result = db.find_css(&FontQuery::new().family("arial"));
    assert!(result.is_some(), "arial (lowercase) must match Arial");
    let result2 = db.find_css(&FontQuery::new().family("ARIAL"));
    assert!(result2.is_some(), "ARIAL (uppercase) must match Arial");
}

// ---------------------------------------------------------------------------
// test_find_css_weight_narrowing
// ---------------------------------------------------------------------------

/// CSS §4.5.5 weight narrowing:
/// - query=400 with db {300, 400, 700} → returns weight 400
/// - query=350 with db {300, 400, 700} → returns weight 300 (nearest below for <400)
#[test]
fn test_find_css_weight_narrowing() {
    let db = FontDatabase::from_faces(vec![
        make_face("Arial", FontStyle::Normal, 300, FontStretch::Normal),
        make_face("Arial", FontStyle::Normal, 400, FontStretch::Normal),
        make_face("Arial", FontStyle::Normal, 700, FontStretch::Normal),
    ]);

    // query=400 → exact match wins
    let r400 = db.find_css(&FontQuery::new().family("Arial").weight(400));
    assert!(r400.is_some(), "weight 400 must be found");
    assert_eq!(
        r400.unwrap().weight,
        400,
        "exact weight 400 must be selected"
    );

    // query=350 → below preferred, nearest below is 300
    let r350 = db.find_css(&FontQuery::new().family("Arial").weight(350));
    assert!(r350.is_some(), "weight 350 must find nearest-below face");
    assert_eq!(r350.unwrap().weight, 300, "weight 350 → nearest-below=300");
}

/// weight=400 with {300, 500, 700} → 500 is preferred (CSS special case)
#[test]
fn test_find_css_weight_400_prefers_500_over_300() {
    let db = FontDatabase::from_faces(vec![
        make_face("Arial", FontStyle::Normal, 300, FontStretch::Normal),
        make_face("Arial", FontStyle::Normal, 500, FontStretch::Normal),
        make_face("Arial", FontStyle::Normal, 700, FontStretch::Normal),
    ]);

    let r = db.find_css(&FontQuery::new().family("Arial").weight(400));
    assert!(r.is_some());
    assert_eq!(
        r.unwrap().weight,
        500,
        "CSS §4.5.5: 400 prefers 500 over 300 when 400 absent"
    );
}

/// weight=500 with {400, 700} → 400 is preferred (CSS special case)
#[test]
fn test_find_css_weight_500_prefers_400() {
    let db = FontDatabase::from_faces(vec![
        make_face("Arial", FontStyle::Normal, 400, FontStretch::Normal),
        make_face("Arial", FontStyle::Normal, 700, FontStretch::Normal),
    ]);

    let r = db.find_css(&FontQuery::new().family("Arial").weight(500));
    assert!(r.is_some());
    assert_eq!(
        r.unwrap().weight,
        400,
        "CSS §4.5.5: 500 prefers 400 when 500 absent"
    );
}

/// weight=600 with {400, 700} → 700 (nearest above, >500 branch)
#[test]
fn test_find_css_weight_600_prefers_above() {
    let db = FontDatabase::from_faces(vec![
        make_face("Arial", FontStyle::Normal, 400, FontStretch::Normal),
        make_face("Arial", FontStyle::Normal, 700, FontStretch::Normal),
    ]);

    let r = db.find_css(&FontQuery::new().family("Arial").weight(600));
    assert!(r.is_some());
    assert_eq!(
        r.unwrap().weight,
        700,
        "CSS §4.5.5: weight 600 → nearest above = 700"
    );
}

// ---------------------------------------------------------------------------
// test_find_css_italic_narrowing
// ---------------------------------------------------------------------------

/// A database with italic and non-italic Arial; italic query returns italic.
#[test]
fn test_find_css_italic_narrowing() {
    let db = FontDatabase::from_faces(vec![
        make_face("Arial", FontStyle::Normal, 400, FontStretch::Normal),
        make_face("Arial", FontStyle::Italic, 400, FontStretch::Normal),
    ]);

    let r = db.find_css(&FontQuery::new().family("Arial").style(FontStyle::Italic));
    assert!(r.is_some(), "italic query must find a face");
    assert_eq!(
        r.unwrap().style,
        FontStyle::Italic,
        "italic query must return italic face"
    );
}

/// Normal style query prefers normal over oblique and italic.
#[test]
fn test_find_css_normal_style_narrowing() {
    let db = FontDatabase::from_faces(vec![
        make_face("Arial", FontStyle::Italic, 400, FontStretch::Normal),
        make_face("Arial", FontStyle::Normal, 400, FontStretch::Normal),
        make_face("Arial", FontStyle::Oblique, 400, FontStretch::Normal),
    ]);

    let r = db.find_css(&FontQuery::new().family("Arial").style(FontStyle::Normal));
    assert!(r.is_some());
    assert_eq!(
        r.unwrap().style,
        FontStyle::Normal,
        "normal query must select normal face"
    );
}

/// Oblique query prefers oblique, then italic, then normal.
#[test]
fn test_find_css_oblique_style_narrowing() {
    let db = FontDatabase::from_faces(vec![
        make_face("Arial", FontStyle::Normal, 400, FontStretch::Normal),
        make_face("Arial", FontStyle::Italic, 400, FontStretch::Normal),
    ]);

    // No oblique face — oblique query should fall to italic as second choice.
    let r = db.find_css(&FontQuery::new().family("Arial").style(FontStyle::Oblique));
    assert!(r.is_some());
    assert_eq!(
        r.unwrap().style,
        FontStyle::Italic,
        "oblique query falls back to italic when no oblique face exists"
    );
}

// ---------------------------------------------------------------------------
// test_find_css_generic_sans_serif
// ---------------------------------------------------------------------------

/// A database containing only "Arial"; querying for "sans-serif" resolves to
/// Arial via the generic alias table.
#[test]
fn test_find_css_generic_sans_serif() {
    let db = FontDatabase::from_faces(vec![make_face(
        "Arial",
        FontStyle::Normal,
        400,
        FontStretch::Normal,
    )]);

    let r = db.find_css(&FontQuery::new().family("sans-serif"));
    assert!(r.is_some(), "sans-serif must resolve to Arial");
    assert_eq!(&*r.unwrap().family, "Arial", "sans-serif resolves to Arial");
}

/// serif generic resolves to "Times New Roman" when present.
#[test]
fn test_find_css_generic_serif() {
    let db = FontDatabase::from_faces(vec![make_face(
        "Times New Roman",
        FontStyle::Normal,
        400,
        FontStretch::Normal,
    )]);

    let r = db.find_css(&FontQuery::new().family("serif"));
    assert!(r.is_some(), "serif generic must resolve");
    assert_eq!(&*r.unwrap().family, "Times New Roman");
}

/// monospace generic resolves to "Courier New" when present.
#[test]
fn test_find_css_generic_monospace() {
    let db = FontDatabase::from_faces(vec![make_face(
        "Courier New",
        FontStyle::Normal,
        400,
        FontStretch::Normal,
    )]);

    let r = db.find_css(&FontQuery::new().family("monospace"));
    assert!(r.is_some(), "monospace generic must resolve");
    assert_eq!(&*r.unwrap().family, "Courier New");
}

// ---------------------------------------------------------------------------
// test_find_css_returns_none_for_missing_family
// ---------------------------------------------------------------------------

#[test]
fn test_find_css_returns_none_for_missing_family() {
    let db = FontDatabase::from_faces(vec![make_face(
        "Arial",
        FontStyle::Normal,
        400,
        FontStretch::Normal,
    )]);

    let r = db.find_css(&FontQuery::new().family("NonExistentFont1234"));
    assert!(r.is_none(), "query for unknown family must return None");
}

// ---------------------------------------------------------------------------
// test_find_css_all_weights
// ---------------------------------------------------------------------------

/// Verify CSS §4.5.5 weight selection for all cases with a 5-weight database.
///
/// Database: Arial at weights 100, 300, 400, 700, 900.
#[test]
fn test_find_css_all_weights() {
    let db = FontDatabase::from_faces(vec![
        make_face("Arial", FontStyle::Normal, 100, FontStretch::Normal),
        make_face("Arial", FontStyle::Normal, 300, FontStretch::Normal),
        make_face("Arial", FontStyle::Normal, 400, FontStretch::Normal),
        make_face("Arial", FontStyle::Normal, 700, FontStretch::Normal),
        make_face("Arial", FontStyle::Normal, 900, FontStretch::Normal),
    ]);

    // query=100 → exact
    let r = db.find_css(&FontQuery::new().family("Arial").weight(100));
    assert_eq!(r.map(|f| f.weight), Some(100));

    // query=400 → exact
    let r = db.find_css(&FontQuery::new().family("Arial").weight(400));
    assert_eq!(r.map(|f| f.weight), Some(400));

    // query=200 → nearest below=100 (below-first for <400)
    let r = db.find_css(&FontQuery::new().family("Arial").weight(200));
    assert_eq!(r.map(|f| f.weight), Some(100));

    // query=500 → 400 preferred (CSS special case for 500: prefer 400 first)
    let r = db.find_css(&FontQuery::new().family("Arial").weight(500));
    assert_eq!(
        r.map(|f| f.weight),
        Some(400),
        "weight=500 → 400 (special case)"
    );

    // query=600 → nearest above=700 (above-first for >500)
    let r = db.find_css(&FontQuery::new().family("Arial").weight(600));
    assert_eq!(r.map(|f| f.weight), Some(700));

    // query=800 → nearest above=900 (above-first for >500)
    let r = db.find_css(&FontQuery::new().family("Arial").weight(800));
    assert_eq!(r.map(|f| f.weight), Some(900));

    // query=950 → no face above 950 (only 900), so nearest-below=900
    let r = db.find_css(&FontQuery::new().family("Arial").weight(950));
    assert_eq!(
        r.map(|f| f.weight),
        Some(900),
        "weight=950 → 900 (nearest below fallback)"
    );
}

// ---------------------------------------------------------------------------
// test_find_css_stretch_narrowing
// ---------------------------------------------------------------------------

/// CSS §4.5.3 stretch narrowing: query S=4 (semi-condensed, ≤5) prefers
/// values ≤ S first (nearest), then > S.
#[test]
fn test_find_css_stretch_narrowing_condensed_query() {
    let db = FontDatabase::from_faces(vec![
        make_face("Arial", FontStyle::Normal, 400, FontStretch::Condensed), // 3
        make_face("Arial", FontStyle::Normal, 400, FontStretch::SemiCondensed), // 4
        make_face("Arial", FontStyle::Normal, 400, FontStretch::Normal),    // 5
        make_face("Arial", FontStyle::Normal, 400, FontStretch::Expanded),  // 7
    ]);

    // query stretch = SemiCondensed (4); exact match should win
    let r = db.find_css(
        &FontQuery::new()
            .family("Arial")
            .stretch(FontStretch::SemiCondensed),
    );
    assert!(r.is_some());
    assert_eq!(
        r.unwrap().stretch,
        FontStretch::SemiCondensed,
        "exact stretch match must win"
    );
}

/// CSS §4.5.3: query S=6 (semi-expanded, >5) prefers values ≥ S first.
#[test]
fn test_find_css_stretch_narrowing_expanded_query() {
    let db = FontDatabase::from_faces(vec![
        make_face("Arial", FontStyle::Normal, 400, FontStretch::Normal), // 5
        make_face("Arial", FontStyle::Normal, 400, FontStretch::Expanded), // 7
    ]);

    // query S=6 (SemiExpanded). No exact match. Nearest above = 7 (Expanded).
    let r = db.find_css(
        &FontQuery::new()
            .family("Arial")
            .stretch(FontStretch::SemiExpanded),
    );
    assert!(r.is_some());
    assert_eq!(
        r.unwrap().stretch,
        FontStretch::Expanded,
        "S=6 query → nearest above (7) preferred when 6 absent"
    );
}

// ---------------------------------------------------------------------------
// test_find_css_case_insensitive_family
// ---------------------------------------------------------------------------

/// Querying with an all-lowercase family name must find a face whose stored
/// family name uses mixed or upper case.  Demonstrates that `find_css` (via
/// `by_family` key normalisation) does case-insensitive exact-name matching.
#[test]
fn test_find_css_case_insensitive_family() {
    // Register "Arial" (capitalised as stored by most font files).
    let db = FontDatabase::from_faces(vec![make_face(
        "Arial",
        FontStyle::Normal,
        400,
        FontStretch::Normal,
    )]);

    // Query with lowercase "arial" — must still find the face.
    let r_lower = db.find_css(&FontQuery::new().family("arial"));
    assert!(
        r_lower.is_some(),
        "lowercase 'arial' must match stored 'Arial'"
    );
    assert_eq!(&*r_lower.unwrap().family, "Arial");

    // Query with all-caps "ARIAL".
    let r_upper = db.find_css(&FontQuery::new().family("ARIAL"));
    assert!(
        r_upper.is_some(),
        "uppercase 'ARIAL' must match stored 'Arial'"
    );
    assert_eq!(&*r_upper.unwrap().family, "Arial");

    // Query with mixed-case "aRiAl".
    let r_mixed = db.find_css(&FontQuery::new().family("aRiAl"));
    assert!(
        r_mixed.is_some(),
        "mixed-case 'aRiAl' must match stored 'Arial'"
    );
}

// ---------------------------------------------------------------------------
// test_find_css_multiple_families
// ---------------------------------------------------------------------------

/// `find_with_fallback` must try families in order: when the first family is
/// absent the second one is returned; when the first is present it wins.
#[test]
fn test_find_css_multiple_families() {
    // Database only has "Helvetica".
    let db_only_helvetica = FontDatabase::from_faces(vec![make_face(
        "Helvetica",
        FontStyle::Normal,
        400,
        FontStretch::Normal,
    )]);

    let base = FontQuery::new();

    // "Arial" is absent; the second family "Helvetica" must be returned.
    let result = db_only_helvetica.find_with_fallback(&["Arial", "Helvetica"], &base, "");
    assert!(
        result.is_some(),
        "second family 'Helvetica' must be found when 'Arial' is absent"
    );
    assert_eq!(&*result.unwrap().family, "Helvetica");

    // Database now has both "Arial" and "Helvetica".
    let db_both = FontDatabase::from_faces(vec![
        make_face("Arial", FontStyle::Normal, 400, FontStretch::Normal),
        make_face("Helvetica", FontStyle::Normal, 400, FontStretch::Normal),
    ]);

    // "Arial" is present as the first family; it must win over "Helvetica".
    let result2 = db_both.find_with_fallback(&["Arial", "Helvetica"], &base, "");
    assert!(result2.is_some(), "first family 'Arial' must be found");
    assert_eq!(
        &*result2.unwrap().family,
        "Arial",
        "first present family in the list must be returned"
    );

    // Neither family present → None.
    let db_empty = FontDatabase::new();
    let result3 = db_empty.find_with_fallback(&["Arial", "Helvetica"], &base, "");
    assert!(
        result3.is_none(),
        "must return None when no family is in db"
    );
}

// ---------------------------------------------------------------------------
// test_find_best_for_text
// ---------------------------------------------------------------------------

/// `find_best_for_text` must behave identically to `find_css` for a single
/// family name, including CSS generic resolution.
#[test]
fn test_find_best_for_text_delegates_to_css() {
    let db = FontDatabase::from_faces(vec![
        make_face("Arial", FontStyle::Normal, 400, FontStretch::Normal),
        make_face("Arial", FontStyle::Italic, 700, FontStretch::Normal),
    ]);

    // Exact family + weight.
    let q = FontQuery::new()
        .family("Arial")
        .weight(700)
        .style(FontStyle::Italic);
    let r = db.find_best_for_text(&q, "Hello");
    assert!(r.is_some());
    assert_eq!(r.unwrap().weight, 700);
    assert_eq!(r.unwrap().style, FontStyle::Italic);

    // Generic family.
    let db2 = FontDatabase::from_faces(vec![make_face(
        "Courier New",
        FontStyle::Normal,
        400,
        FontStretch::Normal,
    )]);
    let q2 = FontQuery::new().family("monospace");
    let r2 = db2.find_best_for_text(&q2, "code");
    assert!(
        r2.is_some(),
        "monospace generic must resolve to Courier New"
    );
    assert_eq!(&*r2.unwrap().family, "Courier New");

    // No family → returns first (wildcard).
    let q3 = FontQuery::new();
    let r3 = db.find_best_for_text(&q3, "any");
    assert!(r3.is_some(), "no family constraint must return some face");
}

// ---------------------------------------------------------------------------
// test_system_catalog_not_empty
// ---------------------------------------------------------------------------

/// Smoke test: `system()` must not panic. We don't assert non-empty because
/// CI containers may have no system fonts installed.
#[test]
fn test_system_catalog_not_empty() {
    let catalog = FontDatabase::system().expect("system() must not error");
    // Verify `len()` does not panic; is_empty() must be consistent with len().
    let n = catalog.len();
    if n == 0 {
        assert!(catalog.is_empty(), "len=0 must imply is_empty");
    } else {
        assert!(!catalog.is_empty(), "len>0 must imply !is_empty");
    }
}
