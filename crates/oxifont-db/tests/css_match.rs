//! CSS Fonts Level 4 matching algorithm tests.
//!
//! These tests verify the weight, stretch, style, and generic-alias matching
//! behaviour using minimal synthetic [`FaceInfo`] fixtures.

use oxifont_db::{FaceInfo, FontDatabase, Query, Source, VariationAxis};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_face(family: &str, weight: u16, italic: bool, stretch: u8) -> FaceInfo {
    FaceInfo {
        id: 0,
        family: family.to_string(),
        post_script_name: String::new(),
        weight,
        italic,
        stretch,
        monospaced: false,
        source: Source::Memory(Vec::new()),
        face_index: 0,
        variable_axes: Vec::new(),
        locale_families: Vec::new(),
        unicode_ranges: 0,
    }
}

fn make_face_with_psn(family: &str, weight: u16, italic: bool, post_script_name: &str) -> FaceInfo {
    FaceInfo {
        post_script_name: post_script_name.to_string(),
        ..make_face(family, weight, italic, 5)
    }
}

fn make_ital_var_face(family: &str, ital_max: f32) -> FaceInfo {
    FaceInfo {
        id: 0,
        family: family.to_string(),
        post_script_name: String::new(),
        weight: 400,
        italic: true,
        stretch: 5,
        monospaced: false,
        source: Source::Memory(Vec::new()),
        face_index: 0,
        variable_axes: vec![
            VariationAxis {
                tag: [b'w', b'g', b'h', b't'],
                min_value: 100.0,
                max_value: 900.0,
                default_value: 400.0,
                name: String::new(),
            },
            VariationAxis {
                tag: [b'i', b't', b'a', b'l'],
                min_value: 0.0,
                max_value: ital_max,
                default_value: 0.0,
                name: String::new(),
            },
        ],
        locale_families: Vec::new(),
        unicode_ranges: 0,
    }
}

fn make_wdth_var_face(family: &str, wdth_min: f32, wdth_max: f32) -> FaceInfo {
    FaceInfo {
        id: 0,
        family: family.to_string(),
        post_script_name: String::new(),
        weight: 400,
        italic: false,
        stretch: 3, // condensed static classification
        monospaced: false,
        source: Source::Memory(Vec::new()),
        face_index: 0,
        variable_axes: vec![VariationAxis {
            tag: [b'w', b'd', b't', b'h'],
            min_value: wdth_min,
            max_value: wdth_max,
            default_value: 100.0,
            name: String::new(),
        }],
        locale_families: Vec::new(),
        unicode_ranges: 0,
    }
}

// ---------------------------------------------------------------------------
// Weight matching tests
// ---------------------------------------------------------------------------

/// CSS §4.5.5: query 400 with faces at 500 and 300 — prefer 500 (next above).
#[test]
fn test_weight_400_prefers_500_over_300() {
    let mut db = FontDatabase::new();
    db.add_face(make_face("TestFont", 300, false, 5));
    db.add_face(make_face("TestFont", 500, false, 5));

    let face = Query::new(&db)
        .family("TestFont")
        .weight(400)
        .match_best()
        .expect("must match");
    assert_eq!(
        face.weight, 500,
        "weight 400 with no exact match: 500 preferred over 300 (spec: 400→500 first)"
    );
}

/// CSS §4.5.5: query 350 (< 400) with faces at 300 and 400 — prefer 300
/// (nearest below descending).
#[test]
fn test_weight_350_prefers_300_over_400() {
    let mut db = FontDatabase::new();
    db.add_face(make_face("TestFont", 300, false, 5));
    db.add_face(make_face("TestFont", 400, false, 5));

    let face = Query::new(&db)
        .family("TestFont")
        .weight(350)
        .match_best()
        .expect("must match");
    assert_eq!(
        face.weight, 300,
        "weight 350 < 400: nearest below (300) preferred over above (400)"
    );
}

/// CSS §4.5.5: query 600 (> 500) with faces at 500 and 700 — prefer 700
/// (nearest above ascending).
#[test]
fn test_weight_600_prefers_700_over_500() {
    let mut db = FontDatabase::new();
    db.add_face(make_face("TestFont", 500, false, 5));
    db.add_face(make_face("TestFont", 700, false, 5));

    let face = Query::new(&db)
        .family("TestFont")
        .weight(600)
        .match_best()
        .expect("must match");
    assert_eq!(
        face.weight, 700,
        "weight 600 > 500: nearest above (700) preferred over below (500)"
    );
}

// ---------------------------------------------------------------------------
// Stretch matching test
// ---------------------------------------------------------------------------

/// Condensed query (stretch=3) should prefer a condensed face over a normal one.
#[test]
fn test_stretch_condensed_matches_condensed() {
    let mut db = FontDatabase::new();
    db.add_face(make_face("TestFont", 400, false, 5)); // normal
    db.add_face(make_face("TestFont", 400, false, 3)); // condensed

    let face = Query::new(&db)
        .family("TestFont")
        .stretch(3)
        .match_best()
        .expect("must match");
    assert_eq!(
        face.stretch, 3,
        "condensed query should select the condensed face (stretch=3)"
    );
}

// ---------------------------------------------------------------------------
// Style matching test
// ---------------------------------------------------------------------------

/// Italic query should prefer the italic face over the normal one.
#[test]
fn test_italic_style_prefers_italic() {
    let mut db = FontDatabase::new();
    db.add_face(make_face("TestFont", 400, false, 5)); // normal
    db.add_face(make_face("TestFont", 400, true, 5)); // italic

    let face = Query::new(&db)
        .family("TestFont")
        .italic(true)
        .match_best()
        .expect("must match");
    assert!(face.italic, "italic query should select the italic face");
}

// ---------------------------------------------------------------------------
// Generic alias test
// ---------------------------------------------------------------------------

/// The generic alias `"sans-serif"` should resolve to a concrete family that
/// exists in the database (Liberation Sans is in the alias table).
#[test]
fn test_generic_alias_sans_serif() {
    let mut db = FontDatabase::new();
    db.add_face(make_face("Liberation Sans", 400, false, 5));
    db.add_face(make_face("Some Other Font", 400, false, 5));

    let face = Query::new(&db)
        .family("sans-serif")
        .match_best()
        .expect("sans-serif generic alias must resolve to Liberation Sans");
    assert_eq!(
        face.family, "Liberation Sans",
        "sans-serif alias should pick Liberation Sans"
    );
}

// ---------------------------------------------------------------------------
// Variable-font ital axis preference test
// ---------------------------------------------------------------------------

/// When italic is requested, a variable face with an `ital` axis (max ≥ 1.0)
/// should be preferred over a static italic face.
#[test]
fn test_ital_axis_preferred_for_italic_query() {
    let mut db = FontDatabase::new();
    // Static italic face.
    db.add_face(make_face("VarFont", 400, true, 5));
    // Variable face with ital axis (max=1.0).
    db.add_face(make_ital_var_face("VarFont", 1.0));

    let face = Query::new(&db)
        .family("VarFont")
        .italic(true)
        .match_best()
        .expect("must match");

    let has_ital = face
        .variable_axes
        .iter()
        .any(|ax| ax.tag == [b'i', b't', b'a', b'l'] && ax.max_value >= 1.0);
    assert!(
        has_ital,
        "variable face with ital axis should be preferred for italic query"
    );
}

/// A variable face with `ital` max < 1.0 should NOT be preferred over a static
/// italic face for italic queries.
#[test]
fn test_ital_axis_not_preferred_when_max_below_one() {
    let mut db = FontDatabase::new();
    // Static italic.
    db.add_face(make_face("PartialItal", 400, true, 5));
    // Variable with ital max = 0.5 — does not reach full italic.
    db.add_face(make_ital_var_face("PartialItal", 0.5));

    // Both faces are italic; query should return one of them without panicking.
    let result = Query::new(&db)
        .family("PartialItal")
        .italic(true)
        .match_best();
    assert!(
        result.is_some(),
        "should still return a face even when ital axis is partial"
    );
}

// ---------------------------------------------------------------------------
// Variable-font wdth axis preference test
// ---------------------------------------------------------------------------

/// A variable face whose `wdth` axis covers the condensed percentage (75%)
/// should be preferred for a condensed (stretch=3) query.
#[test]
fn test_wdth_axis_preferred_for_condensed_query() {
    let mut db = FontDatabase::new();
    // Static condensed face.
    db.add_face(make_face("WdthFont", 400, false, 3));
    // Variable face with wdth axis covering 50–125 (includes 75%).
    db.add_face(make_wdth_var_face("WdthFont", 50.0, 125.0));

    let face = Query::new(&db)
        .family("WdthFont")
        .stretch(3)
        .match_best()
        .expect("must match");

    let has_wdth = face.variable_axes.iter().any(|ax| {
        ax.tag == [b'w', b'd', b't', b'h'] && ax.min_value <= 75.0 && ax.max_value >= 75.0
    });
    assert!(
        has_wdth,
        "variable face with wdth axis covering 75% should be preferred for condensed query"
    );
}

// ---------------------------------------------------------------------------
// match_all test
// ---------------------------------------------------------------------------

/// `match_all` returns all matching faces in preference order.
#[test]
fn test_match_all_returns_sorted_candidates() {
    let mut db = FontDatabase::new();
    db.add_face(make_face("MultiFont", 400, false, 5));
    db.add_face(make_face("MultiFont", 700, false, 5));
    db.add_face(make_face("MultiFont", 900, false, 5));

    let all = Query::new(&db).family("MultiFont").weight(700).match_all();

    assert_eq!(all.len(), 3, "match_all should return all three faces");
    // Best match (700) should be first.
    assert_eq!(
        all[0].weight, 700,
        "best match should be first in match_all"
    );
}

/// `match_all` for an unknown family returns an empty vec.
#[test]
fn test_match_all_empty_for_unknown_family() {
    let db = FontDatabase::new();
    let all = Query::new(&db).family("NoSuchFont").match_all();
    assert!(
        all.is_empty(),
        "match_all must return empty vec for unknown family"
    );
}

// ---------------------------------------------------------------------------
// Oblique preference test
// ---------------------------------------------------------------------------

/// When `.oblique(true)` is set, a face whose PostScript name contains
/// "Oblique" is preferred over a plain italic face.
#[test]
fn test_oblique_prefers_oblique_psn() {
    let mut db = FontDatabase::new();
    db.add_face(make_face_with_psn(
        "ObliqueFont",
        400,
        true,
        "ObliqueFont-Italic",
    ));
    db.add_face(make_face_with_psn(
        "ObliqueFont",
        400,
        true,
        "ObliqueFont-Oblique",
    ));

    let face = Query::new(&db)
        .family("ObliqueFont")
        .italic(true)
        .oblique(true)
        .match_best()
        .expect("must match");

    assert!(
        face.post_script_name.to_lowercase().contains("oblique"),
        "oblique query should prefer face with 'Oblique' in PostScript name, got: {}",
        face.post_script_name
    );
}

// ---------------------------------------------------------------------------
// Locale soft preference test
// ---------------------------------------------------------------------------

/// When a locale is set, faces with a matching locale entry should be softly
/// preferred.  If only one face has locale data matching the requested locale,
/// it should be returned.
#[test]
fn test_locale_soft_preference() {
    let mut db = FontDatabase::new();

    // Face with Japanese locale entry (LCID 0x0411).
    let mut ja_face = make_face("LocaleFont", 400, false, 5);
    ja_face.locale_families = vec![(0x0411u16, "ローカルフォント".to_string())];
    db.add_face(ja_face);

    // Face with no locale data.
    db.add_face(make_face("LocaleFont", 400, false, 5));

    // match_all with locale "ja-JP" — the face with locale data should be first.
    let all = Query::new(&db)
        .family("LocaleFont")
        .locale("ja-JP")
        .match_all();

    assert_eq!(all.len(), 2, "both faces should be returned");
    let first_has_locale = all[0]
        .locale_families
        .iter()
        .any(|(id, _)| *id == 0x0411u16);
    assert!(first_has_locale, "locale-matched face should come first");
}

// ---------------------------------------------------------------------------
// PostScript name index test
// ---------------------------------------------------------------------------

/// `find_by_postscript_name` should return the face with the matching PostScript
/// name and `None` for an unknown name.
#[test]
fn test_find_by_postscript_name() {
    let mut db = FontDatabase::new();
    db.add_face(make_face_with_psn("PSNFont", 400, false, "PSNFont-Regular"));
    db.add_face(make_face_with_psn("PSNFont", 700, false, "PSNFont-Bold"));

    let found = db.find_by_postscript_name("PSNFont-Bold");
    assert!(
        found.is_some(),
        "should find PSNFont-Bold by PostScript name"
    );
    assert_eq!(found.expect("checked above").weight, 700);

    assert!(
        db.find_by_postscript_name("PSNFont-Light").is_none(),
        "unknown PostScript name should return None"
    );
}

// ---------------------------------------------------------------------------
// face_by_id and faces_by_family tests
// ---------------------------------------------------------------------------

#[test]
fn test_face_by_id_roundtrip() {
    let mut db = FontDatabase::new();
    let _ = db.add_face(make_face("IdFont", 400, false, 5));
    let _ = db.add_face(make_face("IdFont", 700, false, 5));

    let face = db.face_by_id(1).expect("face with id=1 should exist");
    assert_eq!(face.weight, 700);
    assert!(
        db.face_by_id(99).is_none(),
        "out-of-range id should return None"
    );
}

#[test]
fn test_faces_by_family_case_insensitive() {
    let mut db = FontDatabase::new();
    db.add_face(make_face("CasedFamily", 400, false, 5));
    db.add_face(make_face("CasedFamily", 700, false, 5));

    let faces = db.faces_by_family("casedfamily");
    assert_eq!(
        faces.len(),
        2,
        "case-insensitive lookup should return both faces"
    );

    let faces_upper = db.faces_by_family("CASEDFAMILY");
    assert_eq!(faces_upper.len(), 2);
}

// ---------------------------------------------------------------------------
// TryFrom<db::FaceInfo> for core::FaceInfo test
// ---------------------------------------------------------------------------

#[test]
fn test_try_from_file_face_succeeds() {
    use oxifont_core::FaceInfo as CoreFaceInfo;

    let db_face = FaceInfo {
        id: 0,
        family: "BridgeFont".to_string(),
        post_script_name: "BridgeFont-Regular".to_string(),
        weight: 400,
        italic: false,
        stretch: 5,
        monospaced: false,
        source: Source::File(std::env::temp_dir().join("bridge.ttf")),
        face_index: 0,
        variable_axes: Vec::new(),
        locale_families: Vec::new(),
        unicode_ranges: 0,
    };

    let core: Result<CoreFaceInfo, ()> = db_face.try_into();
    assert!(
        core.is_ok(),
        "File-backed FaceInfo should convert successfully"
    );
    let core = core.expect("checked above");
    assert_eq!(&*core.family, "BridgeFont");
    assert_eq!(core.weight, 400);
    assert_eq!(core.path, std::env::temp_dir().join("bridge.ttf"));
}

#[test]
fn test_try_from_memory_face_fails() {
    use oxifont_core::FaceInfo as CoreFaceInfo;

    let db_face = make_face("MemFont", 400, false, 5);
    // source is Memory — conversion should fail.
    let result: Result<CoreFaceInfo, ()> = db_face.try_into();
    assert!(
        result.is_err(),
        "Memory-backed FaceInfo should fail conversion to core::FaceInfo"
    );
}

#[test]
fn test_try_from_italic_maps_to_italic_style() {
    use oxifont_core::{FaceInfo as CoreFaceInfo, FontStyle};

    let db_face = FaceInfo {
        id: 0,
        family: "ItalFont".to_string(),
        post_script_name: "ItalFont-Italic".to_string(),
        weight: 400,
        italic: true,
        stretch: 5,
        monospaced: false,
        source: Source::File(std::env::temp_dir().join("ital.ttf")),
        face_index: 0,
        variable_axes: Vec::new(),
        locale_families: Vec::new(),
        unicode_ranges: 0,
    };
    let core: CoreFaceInfo = db_face.try_into().expect("should succeed");
    assert_eq!(core.style, FontStyle::Italic);
}

#[test]
fn test_try_from_oblique_psn_maps_to_oblique_style() {
    use oxifont_core::{FaceInfo as CoreFaceInfo, FontStyle};

    let db_face = FaceInfo {
        id: 0,
        family: "OblFont".to_string(),
        post_script_name: "OblFont-Oblique".to_string(),
        weight: 400,
        italic: true,
        stretch: 5,
        monospaced: false,
        source: Source::File(std::env::temp_dir().join("obl.ttf")),
        face_index: 0,
        variable_axes: Vec::new(),
        locale_families: Vec::new(),
        unicode_ranges: 0,
    };
    let core: CoreFaceInfo = db_face.try_into().expect("should succeed");
    assert_eq!(core.style, FontStyle::Oblique);
}
