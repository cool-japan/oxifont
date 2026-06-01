//! Comprehensive CSS Fonts Level 4 matching tests.
//!
//! Covers weight ordering edge cases (§4.5.5), stretch narrowing (§4.5.3),
//! style narrowing (§4.5.4), generic alias resolution, variable-font
//! preference, locale-specific family lookup, and BCP-47 → LCID mapping.

use oxifont_db::{FaceInfo, FontDatabase, Query, Source, VariationAxis};

// ---------------------------------------------------------------------------
// Shared test helpers
// ---------------------------------------------------------------------------

/// Build a minimal [`FaceInfo`] with the given parameters.
///
/// All optional / derivative fields are zeroed/empty so every call site only
/// needs to supply what the test cares about.
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

/// Build a static face with normal stretch (5) and given italic/weight.
fn make_static(family: &str, weight: u16, italic: bool) -> FaceInfo {
    make_face(family, weight, italic, 5)
}

/// Build a face with a given PostScript name (used for oblique detection).
fn make_face_psn(family: &str, weight: u16, italic: bool, post_script_name: &str) -> FaceInfo {
    FaceInfo {
        post_script_name: post_script_name.to_string(),
        ..make_face(family, weight, italic, 5)
    }
}

/// Build a variable face with a `wght` axis spanning [min, max].
fn make_variable_wght(family: &str, wght_min: f32, wght_max: f32) -> FaceInfo {
    FaceInfo {
        weight: 400,
        variable_axes: vec![VariationAxis {
            tag: [b'w', b'g', b'h', b't'],
            min_value: wght_min,
            max_value: wght_max,
            default_value: 400.0,
            name: String::new(),
        }],
        ..make_static(family, 400, false)
    }
}

/// Build a variable face with both a `wght` and an `ital` axis.
fn make_variable_ital(family: &str, ital_max: f32) -> FaceInfo {
    FaceInfo {
        italic: true,
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
        ..make_static(family, 400, true)
    }
}

// ---------------------------------------------------------------------------
// Group 1 — CSS Weight Ordering Edge Cases (§4.5.5)
//
// Database: weights 100, 300, 400, 500, 700, 900; all same family,
// stretch=5, non-italic.
// ---------------------------------------------------------------------------

/// Populate a db with faces at weights 100, 300, 400, 500, 700, 900
/// (same family, normal stretch, non-italic).
fn db_all_weights(family: &str) -> FontDatabase {
    let mut db = FontDatabase::new();
    for &w in &[100u16, 300, 400, 500, 700, 900] {
        db.add_face(make_static(family, w, false));
    }
    db
}

/// CSS §4.5.5: weight=400 with no w400 face → should pick w500 before w300.
///
/// Per spec the order for w=400 is: 400, 500, <400 descending, >500 ascending.
/// When w400 is absent: 500 wins over 300.
#[test]
fn test_css_weight_400_prefers_500_over_300() {
    let mut db = FontDatabase::new();
    // Deliberately omit w400.
    for &w in &[100u16, 300, 500, 700, 900] {
        db.add_face(make_static("WEdge", w, false));
    }

    let face = Query::new(&db)
        .family("WEdge")
        .weight(400)
        .match_best()
        .expect("must match a face");

    assert_eq!(
        face.weight, 500,
        "CSS §4.5.5: weight=400, no w400 → prefer 500 over 300 (got {})",
        face.weight
    );
}

/// CSS §4.5.5: weight=350 (< 400) — nearest below first, then ascending above.
///
/// With weights 100, 300, 400 available: 300 is the nearest below 350, so
/// it must win over 400 (nearest above).
#[test]
fn test_css_weight_350_picks_nearest_below() {
    let db = db_all_weights("W350");

    let face = Query::new(&db)
        .family("W350")
        .weight(350)
        .match_best()
        .expect("must match a face");

    assert_eq!(
        face.weight, 300,
        "CSS §4.5.5: weight=350 < 400 → nearest below (300) beats nearest above (400), got {}",
        face.weight
    );
}

/// CSS §4.5.5: weight=600 (> 500) — nearest above ascending first.
///
/// With weights 500 and 700: 700 is above 600 and must win over 500.
#[test]
fn test_css_weight_600_picks_nearest_above() {
    let db = db_all_weights("W600");

    let face = Query::new(&db)
        .family("W600")
        .weight(600)
        .match_best()
        .expect("must match a face");

    assert_eq!(
        face.weight, 700,
        "CSS §4.5.5: weight=600 > 500 → nearest above (700) beats nearest below (500), got {}",
        face.weight
    );
}

// ---------------------------------------------------------------------------
// Group 2 — Stretch Narrowing (CSS §4.5.3)
// ---------------------------------------------------------------------------

/// Query stretch=3 (condensed) picks the condensed face over the normal one.
#[test]
fn test_stretch_condensed_over_normal() {
    let mut db = FontDatabase::new();
    db.add_face(make_face("StretchFont", 400, false, 5)); // normal
    db.add_face(make_face("StretchFont", 400, false, 3)); // condensed

    let face = Query::new(&db)
        .family("StretchFont")
        .stretch(3)
        .match_best()
        .expect("must match a face");

    assert_eq!(
        face.stretch, 3,
        "stretch=3 query must select the condensed face (stretch=3), got {}",
        face.stretch
    );
}

/// CSS §4.5.3: query stretch=3 (ultra-condensed direction) with only normal (5)
/// and wide (7) available.
///
/// When no face is at-or-below the requested stretch, the spec says take the
/// nearest above.  So stretch=5 (distance=2) beats stretch=7 (distance=4).
#[test]
fn test_stretch_fallback_order_below_request() {
    let mut db = FontDatabase::new();
    db.add_face(make_face("FallbackStretch", 400, false, 5)); // normal  (above 3, dist 2)
    db.add_face(make_face("FallbackStretch", 400, false, 7)); // wide    (above 3, dist 4)

    let face = Query::new(&db)
        .family("FallbackStretch")
        .stretch(3)
        .match_best()
        .expect("must match a face");

    // Per CSS §4.5.3: requested ≤ 5 → at-or-below descending, then above
    // ascending.  No face is ≤ 3, so take nearest above ascending → 5.
    assert_eq!(
        face.stretch, 5,
        "with no face at/below stretch=3, nearest above ascending (5) must win over (7), got {}",
        face.stretch
    );
}

// ---------------------------------------------------------------------------
// Group 3 — Style Narrowing (CSS §4.5.4)
// ---------------------------------------------------------------------------

/// Query italic=true picks the italic face over the normal face.
#[test]
fn test_italic_picks_italic() {
    let mut db = FontDatabase::new();
    db.add_face(make_static("StyleFont", 400, false)); // normal
    db.add_face(make_static("StyleFont", 400, true)); // italic

    let face = Query::new(&db)
        .family("StyleFont")
        .italic(true)
        .match_best()
        .expect("must match a face");

    assert!(face.italic, "italic query must select the italic face");
}

/// Query italic=true with `.oblique(true)` picks the face whose PostScript name
/// contains "Oblique" over a plain italic face.
///
/// The oblique tiebreaker in query.rs fires when `.oblique(true)` is set.
/// Both faces are italic; the one with "Oblique" in its PostScript name wins.
#[test]
fn test_italic_fallback_to_oblique() {
    let mut db = FontDatabase::new();
    // Plain italic (no "Oblique" in PostScript name).
    db.add_face(make_face_psn("ObFallback", 400, true, "ObFallback-Italic"));
    // Oblique variant (italic=true + "Oblique" in PostScript name).
    db.add_face(make_face_psn("ObFallback", 400, true, "ObFallback-Oblique"));

    let face = Query::new(&db)
        .family("ObFallback")
        .italic(true)
        .oblique(true)
        .match_best()
        .expect("must match a face");

    assert!(
        face.post_script_name.to_lowercase().contains("oblique"),
        "oblique preference: face with 'Oblique' in PostScript name must win, got '{}'",
        face.post_script_name
    );
}

// ---------------------------------------------------------------------------
// Group 4 — Generic Alias Resolution
// ---------------------------------------------------------------------------

/// `"sans-serif"` expands to the fontconfig alias list.
/// Verify that faces whose families appear in that list are returned by
/// `match_all()`.  We populate the db with "Arial" and "DejaVu Sans", both
/// of which are in the GENERIC_ALIASES sans-serif list.
#[test]
fn test_sans_serif_resolves() {
    let mut db = FontDatabase::new();
    db.add_face(make_static("Arial", 400, false));
    db.add_face(make_static("DejaVu Sans", 400, false));
    db.add_face(make_static("NotASansFont", 400, false));

    let all = Query::new(&db).family("sans-serif").match_all();

    assert!(
        !all.is_empty(),
        "sans-serif generic alias must resolve to at least one face"
    );
    // All returned faces must come from the alias-expanded families.
    for face in &all {
        let fam = face.family.as_str();
        assert!(
            fam == "Arial" || fam == "DejaVu Sans",
            "sans-serif resolved a face outside the alias list: '{fam}'"
        );
    }
    // "NotASansFont" must NOT appear.
    assert!(
        all.iter().all(|f| f.family != "NotASansFont"),
        "generic alias resolution must exclude 'NotASansFont'"
    );
}

/// The `"sans-serif"` list includes at least "Arial" and "DejaVu Sans".
#[test]
fn test_sans_serif_includes_arial_and_dejavu() {
    let mut db = FontDatabase::new();
    db.add_face(make_static("Arial", 400, false));
    db.add_face(make_static("DejaVu Sans", 400, false));
    db.add_face(make_static("Helvetica", 400, false));
    db.add_face(make_static("Liberation Sans", 400, false));

    let families: Vec<&str> = Query::new(&db)
        .family("sans-serif")
        .match_all()
        .into_iter()
        .map(|f| f.family.as_str())
        .collect();

    for expected in &["Arial", "DejaVu Sans", "Helvetica", "Liberation Sans"] {
        assert!(
            families.contains(expected),
            "sans-serif alias must include '{expected}'; got {families:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Group 5 — Variable Font Preference
// ---------------------------------------------------------------------------

/// A variable font with a `wght` axis covering [100, 900] is preferred over a
/// static w400 face when querying weight=400.
///
/// Both pass weight-exact match for 400; the variable-axis tiebreaker
/// (`axis_miss` = 0 vs 4) promotes the variable font.
#[test]
fn test_variable_font_preferred_for_weight() {
    let mut db = FontDatabase::new();
    // Static w400 face (no axes → covers_weight returns false → axis_miss=4).
    db.add_face(make_static("VarPref", 400, false));
    // Variable font covering 100–900 → covers_weight(400)=true → axis_miss=0.
    db.add_face(make_variable_wght("VarPref", 100.0, 900.0));

    let face = Query::new(&db)
        .family("VarPref")
        .weight(400)
        .match_best()
        .expect("must match a face");

    assert!(
        face.covers_weight(400),
        "variable font (wght 100-900) must be preferred over static w400 for weight=400 query; \
         got face with axes={:?}",
        face.variable_axes
    );
}

/// A variable font with an `ital` axis (max ≥ 1.0) is preferred over a static
/// italic face when querying italic=true.
#[test]
fn test_variable_font_preferred_for_italic() {
    let mut db = FontDatabase::new();
    // Static italic face (no ital axis → ital_ok=false → axis_miss += 2).
    db.add_face(make_static("VarItal", 400, true));
    // Variable face with ital axis max=1.0 → ital_ok=true → axis_miss stays 0.
    db.add_face(make_variable_ital("VarItal", 1.0));

    let face = Query::new(&db)
        .family("VarItal")
        .italic(true)
        .match_best()
        .expect("must match a face");

    let has_ital_axis = face
        .variable_axes
        .iter()
        .any(|ax| ax.tag == [b'i', b't', b'a', b'l'] && ax.max_value >= 1.0);
    assert!(
        has_ital_axis,
        "variable font with ital axis (max=1.0) must be preferred over static italic for \
         italic=true query; got axes={:?}",
        face.variable_axes
    );
}

// ---------------------------------------------------------------------------
// Group 6 — Locale-Specific Family Name Lookup
// ---------------------------------------------------------------------------

/// A face with a locale entry for "ja-JP" (LCID 0x0411) is softly preferred
/// when querying with `.locale("ja-JP")`.
///
/// The primary `family` field must still match the query family name, because
/// locale_families is a soft tiebreaker, not a lookup key.
#[test]
fn test_locale_selects_localized_face() {
    const JA_LCID: u16 = 0x0411; // ja-JP

    let mut db = FontDatabase::new();

    // Face with Japanese locale entry.
    let ja_face = FaceInfo {
        locale_families: vec![(JA_LCID, "ヒラギノ角ゴシック".to_string())],
        ..make_static("Hiragino Sans", 400, false)
    };
    db.add_face(ja_face);

    // Control face — same family, no locale data.
    db.add_face(make_static("Hiragino Sans", 400, false));

    let all = Query::new(&db)
        .family("Hiragino Sans")
        .locale("ja-JP")
        .match_all();

    assert_eq!(all.len(), 2, "both faces must be returned");

    let first_has_locale = all[0].locale_families.iter().any(|(id, _)| *id == JA_LCID);
    assert!(
        first_has_locale,
        "locale-tagged face must be ranked first when locale='ja-JP' is requested"
    );
}

/// `family_for_locale` returns the locale-specific family name when available.
#[test]
fn test_family_for_locale_returns_localized_name() {
    const JA_LCID: u16 = 0x0411;
    let face = FaceInfo {
        locale_families: vec![(JA_LCID, "ヒラギノ角ゴシック".to_string())],
        ..make_static("Hiragino Sans", 400, false)
    };

    let name = face.family_for_locale("ja-JP");
    assert_eq!(
        name, "ヒラギノ角ゴシック",
        "family_for_locale('ja-JP') must return the Japanese name"
    );

    let fallback = face.family_for_locale("en-US");
    assert_eq!(
        fallback, "Hiragino Sans",
        "family_for_locale for unregistered locale must fall back to primary family"
    );
}

// ---------------------------------------------------------------------------
// Group 7 — BCP-47 to LCID Mapping
// ---------------------------------------------------------------------------

/// Spot-check representative BCP-47 codes from the static table against
/// their expected Windows LCID values.
///
/// This is a data-integrity test — it guards against typos in the mapping
/// and verifies that the lookup function handles case-insensitive input.
#[test]
fn test_bcp47_lcid_known_mappings() {
    use oxifont_db::locale::bcp47_to_lcid;

    // (bcp47, expected_lcid) pairs drawn from locale.rs BCP47_TO_LCID table.
    let cases: &[(&str, u16)] = &[
        ("ja-JP", 0x0411),
        ("ko-KR", 0x0412),
        ("zh-CN", 0x0804),
        ("zh-TW", 0x0404),
        ("zh-HK", 0x0C04),
        ("ar-SA", 0x0401),
        ("fr-FR", 0x040C),
        ("de-DE", 0x0407),
        ("es-ES", 0x0C0A),
        ("en-US", 0x0409),
        ("en-GB", 0x0809),
        ("ru-RU", 0x0419),
    ];

    for &(tag, expected) in cases {
        let lcid = bcp47_to_lcid(tag);
        assert_eq!(
            lcid,
            Some(expected),
            "bcp47_to_lcid({tag:?}) expected Some({expected:#06X}), got {lcid:?}"
        );
    }
}

/// All BCP-47 entries in the static table map to nonzero LCID values.
#[test]
fn test_bcp47_lcid_all_nonzero() {
    use oxifont_db::locale::bcp47_to_lcid;

    // Enumerate all tags known to appear in the static table.
    let all_tags: &[&str] = &[
        "en-us",
        "en-gb",
        "en-au",
        "en-ca",
        "ja-jp",
        "ko-kr",
        "zh-cn",
        "zh-tw",
        "zh-hk",
        "de-de",
        "de-at",
        "de-ch",
        "fr-fr",
        "fr-be",
        "fr-ch",
        "fr-ca",
        "es-es",
        "es-mx",
        "es-ar",
        "it-it",
        "pt-pt",
        "pt-br",
        "ru-ru",
        "ar-sa",
        "pl-pl",
        "nl-nl",
        "nl-be",
        "sv-se",
        "da-dk",
        "fi-fi",
        "nb-no",
        "nn-no",
        "cs-cz",
        "hu-hu",
        "ro-ro",
        "sk-sk",
        "uk-ua",
        "bg-bg",
        "hr-hr",
        "lt-lt",
        "lv-lv",
        "et-ee",
        "tr-tr",
        "vi-vn",
        "th-th",
        "id-id",
        "ms-my",
        "el-gr",
        "he-il",
        "fa-ir",
        "ur-pk",
        "hi-in",
        "bn-in",
        "ta-in",
        "te-in",
        "mr-in",
        "kn-in",
        "ml-in",
        "sr-latn-rs",
        "sr-cyrl-rs",
        "ca-es",
        "gl-es",
        "eu-es",
    ];

    for &tag in all_tags {
        let lcid = bcp47_to_lcid(tag);
        assert!(
            lcid.is_some_and(|v| v != 0),
            "bcp47_to_lcid({tag:?}) expected a nonzero LCID, got {lcid:?}"
        );
    }
}

/// Unknown or invented tags return `None`.
#[test]
fn test_bcp47_lcid_unknown_returns_none() {
    use oxifont_db::locale::bcp47_to_lcid;

    for &tag in &["xx-ZZ", "zz", "tlh", "und"] {
        assert_eq!(
            bcp47_to_lcid(tag),
            None,
            "bcp47_to_lcid({tag:?}) must return None for unknown tags"
        );
    }
}

/// Case-insensitive lookup: upper-case variants must resolve to the same LCID.
#[test]
fn test_bcp47_lcid_case_insensitive() {
    use oxifont_db::locale::bcp47_to_lcid;

    assert_eq!(bcp47_to_lcid("JA-JP"), bcp47_to_lcid("ja-jp"));
    assert_eq!(bcp47_to_lcid("ZH-CN"), bcp47_to_lcid("zh-cn"));
    assert_eq!(bcp47_to_lcid("FR-FR"), bcp47_to_lcid("fr-fr"));
}
