//! Integration tests for the CSS Level 4 weight-matching algorithm and the
//! basic database load-from-bytes path.

use oxifont_db::{FaceInfo, FontDatabase, Query, Source, VariationAxis};

/// Fixture bytes for Noto Sans Regular (OFL licence).
static FIXTURE_BYTES: &[u8] = include_bytes!("fixtures/test.ttf");

// ---------------------------------------------------------------------------
// Helper: build a synthetic FaceInfo for unit tests
// ---------------------------------------------------------------------------

fn make_face(id: u32, family: &str, weight: u16, italic: bool) -> FaceInfo {
    FaceInfo {
        id,
        family: family.to_string(),
        post_script_name: String::new(),
        weight,
        italic,
        stretch: 5,
        monospaced: false,
        source: Source::Memory(Vec::new()),
        face_index: 0,
        variable_axes: Vec::new(),
        locale_families: Vec::new(),
        unicode_ranges: 0,
    }
}

fn make_variable_face(id: u32, family: &str, wght_min: f32, wght_max: f32) -> FaceInfo {
    FaceInfo {
        id,
        family: family.to_string(),
        post_script_name: String::new(),
        weight: 400,
        italic: false,
        stretch: 5,
        monospaced: false,
        source: Source::Memory(Vec::new()),
        face_index: 0,
        variable_axes: vec![VariationAxis {
            tag: [b'w', b'g', b'h', b't'],
            min_value: wght_min,
            max_value: wght_max,
            default_value: 400.0,
            name: String::new(),
        }],
        locale_families: Vec::new(),
        unicode_ranges: 0,
    }
}

// ---------------------------------------------------------------------------
// Test: load real fixture from bytes
// ---------------------------------------------------------------------------

#[test]
fn load_bytes_produces_at_least_one_face() {
    let mut db = FontDatabase::new();
    let added = db.load_bytes(FIXTURE_BYTES.to_vec());
    assert!(
        added > 0,
        "loading fixture bytes must produce at least one face"
    );
    assert!(
        !db.faces().is_empty(),
        "faces() must not be empty after load"
    );
}

#[test]
fn fixture_family_name_non_empty() {
    let mut db = FontDatabase::new();
    db.load_bytes(FIXTURE_BYTES.to_vec());
    let face = &db.faces()[0];
    assert!(!face.family.is_empty(), "family name must not be empty");
}

// ---------------------------------------------------------------------------
// CSS L4 weight algorithm tests
// ---------------------------------------------------------------------------

fn db_with_test_weights() -> FontDatabase {
    let mut db = FontDatabase::new();
    // Add three faces: weights 100, 400, 900.
    for (idx, &weight) in [100u16, 400, 900].iter().enumerate() {
        db.add_face(make_face(idx as u32, "Test", weight, false));
    }
    db
}

#[test]
fn weight_300_query_returns_nearest_below_100() {
    let db = db_with_test_weights();
    let result = Query::new(&db).family("Test").weight(300).match_best();
    let face = result.expect("query must match at least one face");
    // Requested 300 (< 400): nearest below (100), then nearest above (400).
    // 100 is below 300, 400 is above.  Nearest below = 100.
    // Per CSS: <400 descending → 100 is first below candidate.
    assert_eq!(
        face.weight, 100,
        "weight 300 should match nearest below (100), got {}",
        face.weight
    );
}

#[test]
fn weight_600_query_returns_weight_900() {
    let db = db_with_test_weights();
    let result = Query::new(&db).family("Test").weight(600).match_best();
    let face = result.expect("query must match at least one face");
    // Requested 600 (> 500): nearest above ascending → 900 (400 is not > 500 either… wait).
    // Available: 100, 400, 900.  >500: only 900.  Below: 100, 400.
    // So nearest above = 900.
    assert_eq!(
        face.weight, 900,
        "weight 600 should match nearest above (900), got {}",
        face.weight
    );
}

#[test]
fn weight_400_exact_match() {
    let db = db_with_test_weights();
    let result = Query::new(&db).family("Test").weight(400).match_best();
    let face = result.expect("query must match exact weight 400");
    assert_eq!(
        face.weight, 400,
        "weight 400 should get exact match, got {}",
        face.weight
    );
}

#[test]
fn weight_500_prefers_500_over_400() {
    let mut db = FontDatabase::new();
    db.add_face(make_face(0, "Font500", 400, false));
    db.add_face(make_face(1, "Font500", 500, false));
    db.add_face(make_face(2, "Font500", 700, false));

    let result = Query::new(&db).family("Font500").weight(500).match_best();
    let face = result.expect("must find a face");
    assert_eq!(face.weight, 500);
}

#[test]
fn weight_400_prefers_400_then_500() {
    let mut db = FontDatabase::new();
    db.add_face(make_face(0, "FontPref", 500, false));
    db.add_face(make_face(1, "FontPref", 300, false));

    let result = Query::new(&db).family("FontPref").weight(400).match_best();
    let face = result.expect("must find a face");
    // No exact 400; per spec weight==400 order: 400(missing), 500, <400 desc, >500 asc.
    // 500 is available.
    assert_eq!(face.weight, 500);
}

// ---------------------------------------------------------------------------
// Variable-font weight coverage test
// ---------------------------------------------------------------------------

#[test]
fn variable_font_preferred_over_static_when_covering_weight() {
    let mut db = FontDatabase::new();
    // Static face at weight 400.
    db.add_face(make_face(0, "VarTest", 400, false));
    // Variable face covering 100–900.
    db.add_face(make_variable_face(1, "VarTest", 100.0, 900.0));

    let result = Query::new(&db).family("VarTest").weight(450).match_best();
    let face = result.expect("must find a face");
    // The variable face covers weight 450; it should be preferred.
    assert!(
        face.covers_weight(450),
        "returned face should cover weight 450; got static={}",
        face.variable_axes.is_empty()
    );
}
