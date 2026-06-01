//! Tests for generic-alias resolution and variable-font weight preference.

use oxifont_db::{FaceInfo, FontDatabase, Query, Source, VariationAxis};

fn make_static_face(family: &str, weight: u16, monospaced: bool) -> FaceInfo {
    FaceInfo {
        id: 0,
        family: family.to_string(),
        post_script_name: String::new(),
        weight,
        italic: false,
        stretch: 5,
        monospaced,
        source: Source::Memory(Vec::new()),
        face_index: 0,
        variable_axes: Vec::new(),
        locale_families: Vec::new(),
        unicode_ranges: 0,
    }
}

fn make_variable_face(family: &str, wght_min: f32, wght_max: f32) -> FaceInfo {
    FaceInfo {
        id: 0,
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
// Generic alias: "monospace" resolves to Liberation Mono family
// ---------------------------------------------------------------------------

#[test]
fn generic_monospace_resolves_to_liberation_mono() {
    let mut db = FontDatabase::new();
    db.add_face(make_static_face("Liberation Mono", 400, true));
    db.add_face(make_static_face("Arial", 400, false));

    let result = Query::new(&db).family("monospace").weight(400).match_best();

    let face = result.expect("monospace alias must resolve to Liberation Mono");
    assert_eq!(
        face.family, "Liberation Mono",
        "generic 'monospace' should pick Liberation Mono from DB"
    );
}

// ---------------------------------------------------------------------------
// Variable-font preference over static at same weight
// ---------------------------------------------------------------------------

#[test]
fn variable_font_preferred_over_static_when_covering_requested_weight() {
    let mut db = FontDatabase::new();
    // Static face at weight 400.
    db.add_face(make_static_face("Hybrid", 400, false));
    // Variable face (wght 100–900).
    db.add_face(make_variable_face("Hybrid", 100.0, 900.0));

    let result = Query::new(&db).family("Hybrid").weight(450).match_best();
    let face = result.expect("must match a face");
    assert!(
        face.covers_weight(450),
        "the variable face (wght 100-900) should be preferred; \
        returned face variable_axes={:?}",
        face.variable_axes
    );
}

// ---------------------------------------------------------------------------
// Static face returned when variable font does NOT cover requested weight
// ---------------------------------------------------------------------------

#[test]
fn static_face_returned_when_variable_does_not_cover() {
    let mut db = FontDatabase::new();
    // Static face at weight 700.
    db.add_face(make_static_face("Partial", 700, false));
    // Variable face covering only 100–300.
    db.add_face(make_variable_face("Partial", 100.0, 300.0));

    let result = Query::new(&db).family("Partial").weight(700).match_best();
    let face = result.expect("must match a face");
    assert_eq!(
        face.weight, 700,
        "static 700 should win when variable doesn't cover 700"
    );
}

// ---------------------------------------------------------------------------
// No match for unknown family
// ---------------------------------------------------------------------------

#[test]
fn no_match_for_unknown_family() {
    let db = FontDatabase::new();
    let result = Query::new(&db).family("NonExistentFamily").match_best();
    assert!(result.is_none(), "unknown family should return None");
}
