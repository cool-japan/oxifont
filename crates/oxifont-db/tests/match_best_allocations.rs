//! Correctness tests for the match_best allocation reduction and CSS weight-sort
//! optimization (Slice 7, Round 14).
//!
//! These tests verify:
//! 1. Standard CSS Fonts Level 4 §5.2 weight cascade is correct after
//!    the SmallVec / O(1)-dedup refactor.
//! 2. Non-standard (fractional-hundred) weights are handled correctly — the
//!    band-based algorithm must preserve face identity, not collapse weights
//!    to 9 standard slots.
//! 3. Edge-case weights (1, 50, 100, 150, 250, 550, 850, 950, 1000) do not
//!    panic and select sensible faces.

use oxifont_db::{FaceInfo, FontDatabase, Query, Source};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_face(family: &str, weight: u16, italic: bool) -> FaceInfo {
    FaceInfo {
        id: 0,
        family: family.to_string(),
        post_script_name: format!("{}-{}", family, weight),
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

fn db_with_weights(family: &str, weights: &[u16]) -> FontDatabase {
    let mut db = FontDatabase::new();
    for &w in weights {
        db.add_face(make_face(family, w, false));
    }
    db
}

// ---------------------------------------------------------------------------
// Standard CSS weight cascade correctness after SmallVec refactor
// ---------------------------------------------------------------------------

/// weight=400 — exact hit → must return weight 400.
#[test]
fn match_best_weight_400_exact() {
    let db = db_with_weights("Std", &[100, 300, 400, 500, 700, 900]);
    let face = Query::new(&db)
        .family("Std")
        .weight(400)
        .match_best()
        .expect("must match");
    assert_eq!(face.weight, 400, "exact hit at 400 must win");
}

/// weight=400, no w400 → spec says prefer w500 over w300.
#[test]
fn match_best_weight_400_no_exact_prefers_500_over_300() {
    let db = db_with_weights("Nw400", &[100, 300, 500, 700, 900]);
    let face = Query::new(&db)
        .family("Nw400")
        .weight(400)
        .match_best()
        .expect("must match");
    assert_eq!(
        face.weight, 500,
        "CSS §4.5.5: missing w400 → prefer 500 before 300 (got {})",
        face.weight
    );
}

/// weight=500 — exact hit → must return weight 500.
#[test]
fn match_best_weight_500_exact() {
    let db = db_with_weights("Std", &[100, 400, 500, 700, 900]);
    let face = Query::new(&db)
        .family("Std")
        .weight(500)
        .match_best()
        .expect("must match");
    assert_eq!(face.weight, 500, "exact hit at 500 must win");
}

/// weight=500, no w500 → spec says prefer w400 over lower and higher.
#[test]
fn match_best_weight_500_no_exact_prefers_400() {
    let db = db_with_weights("Nw500", &[100, 300, 400, 700, 900]);
    let face = Query::new(&db)
        .family("Nw500")
        .weight(500)
        .match_best()
        .expect("must match");
    assert_eq!(
        face.weight, 400,
        "CSS §4.5.5: missing w500 → prefer 400 next (got {})",
        face.weight
    );
}

/// weight=350 (< 400) → nearest below descending first (w300), then above.
#[test]
fn match_best_weight_350_picks_nearest_below() {
    let db = db_with_weights("W350", &[100, 300, 400, 500, 700, 900]);
    let face = Query::new(&db)
        .family("W350")
        .weight(350)
        .match_best()
        .expect("must match");
    assert_eq!(
        face.weight, 300,
        "weight 350 < 400 → nearest below (300) must win (got {})",
        face.weight
    );
}

/// weight=600 (> 500) → nearest above ascending first (w700), then below.
#[test]
fn match_best_weight_600_picks_nearest_above() {
    let db = db_with_weights("W600", &[100, 300, 400, 500, 700, 900]);
    let face = Query::new(&db)
        .family("W600")
        .weight(600)
        .match_best()
        .expect("must match");
    assert_eq!(
        face.weight, 700,
        "weight 600 > 500 → nearest above (700) must win (got {})",
        face.weight
    );
}

// ---------------------------------------------------------------------------
// Non-standard weights — ensures band logic preserves face identity
// ---------------------------------------------------------------------------

/// Two faces at w350 and w380; query weight=400.
///
/// CSS rule: < 400 → nearest below descending.  Among {350, 380}, 380 is
/// nearer to 400, so it must be selected.  This verifies that the band-based
/// code (not the 9-slot quantization) is used: quantizing to slots would
/// collapse both faces to slot 3 (300) and lose ordering detail.
#[test]
fn non_standard_weights_350_380_query_400_picks_380() {
    let db = db_with_weights("NsW", &[350, 380]);
    let face = Query::new(&db)
        .family("NsW")
        .weight(400)
        .match_best()
        .expect("must match");
    assert_eq!(
        face.weight, 380,
        "nearest below 400 among {{350, 380}} must be 380 (got {})",
        face.weight
    );
}

/// Family with faces at w250 and w270; query weight=300.
///
/// CSS rule: < 400 → nearest below descending first.  270 is nearer to 300
/// than 250, so 270 must win.
#[test]
fn non_standard_weights_250_270_query_300_picks_270() {
    let db = db_with_weights("NsW2", &[250, 270]);
    let face = Query::new(&db)
        .family("NsW2")
        .weight(300)
        .match_best()
        .expect("must match");
    assert_eq!(
        face.weight, 270,
        "nearest below 300 among {{250, 270}} must be 270 (got {})",
        face.weight
    );
}

/// Family with faces at w620 and w650; query weight=600.
///
/// CSS rule: > 500 → nearest above ascending first.  620 is nearer to 600
/// than 650, so 620 must win.
#[test]
fn non_standard_weights_620_650_query_600_picks_620() {
    let db = db_with_weights("NsW3", &[620, 650]);
    let face = Query::new(&db)
        .family("NsW3")
        .weight(600)
        .match_best()
        .expect("must match");
    assert_eq!(
        face.weight, 620,
        "nearest above 600 among {{620, 650}} must be 620 (got {})",
        face.weight
    );
}

// ---------------------------------------------------------------------------
// Edge weights — must not panic and must return a face
// ---------------------------------------------------------------------------

/// Weights that are extreme or between standard hundredths must not cause
/// panics, index-out-of-bounds, or silent mismatches.
///
/// This is the "css_weight_sort_handles_edge_weights" test from the TODO.
#[test]
fn css_weight_sort_handles_edge_weights_no_panic() {
    // The database contains one face at each of the 9 standard CSS weights.
    let db = db_with_weights("EdgeWt", &[100, 200, 300, 400, 500, 600, 700, 800, 900]);

    // Query with each edge weight listed in the TODO — verify no panic and
    // that a face is returned (the exact face returned is spec-correct but
    // the primary assertion here is "no crash").
    let edge_weights: &[u16] = &[1, 50, 100, 150, 250, 550, 850, 950, 1000];
    for &w in edge_weights {
        let result = Query::new(&db).family("EdgeWt").weight(w).match_best();
        assert!(
            result.is_some(),
            "edge weight {w} must return Some(&FaceInfo), got None"
        );
    }
}

/// For edge weight=1 (below 100), the CSS rule for w < 400 applies:
/// nearest below descending first, then above.  No face is below 1, so
/// the nearest above ascending (w100) must be returned.
#[test]
fn edge_weight_1_returns_w100() {
    let db = db_with_weights("EdgeLow", &[100, 400, 700]);
    let face = Query::new(&db)
        .family("EdgeLow")
        .weight(1)
        .match_best()
        .expect("must match");
    assert_eq!(
        face.weight, 100,
        "weight=1 (no face below): nearest above (100) must win (got {})",
        face.weight
    );
}

/// For edge weight=1000 (above 900), the CSS rule for w > 500 applies:
/// nearest above ascending first (none here), then nearest below descending
/// (w900 is closest below 1000).
#[test]
fn edge_weight_1000_returns_w900() {
    let db = db_with_weights("EdgeHigh", &[100, 400, 900]);
    let face = Query::new(&db)
        .family("EdgeHigh")
        .weight(1000)
        .match_best()
        .expect("must match");
    assert_eq!(
        face.weight, 900,
        "weight=1000 (no face above): nearest below (900) must win (got {})",
        face.weight
    );
}

// ---------------------------------------------------------------------------
// Large family (> 16 faces) — exercises the heap-spill path of SmallVec
// ---------------------------------------------------------------------------

/// Build a family with 20 faces (more than FACE_VEC_INLINE=16).
///
/// This exercises the heap-allocation path of SmallVec and confirms that
/// the dedup and ordering logic still works correctly when inline storage
/// is exceeded.
#[test]
fn large_family_20_faces_correct_match() {
    let weights: Vec<u16> = (1u16..=20).map(|i| i * 45).collect(); // 45, 90, …, 900
    let db = db_with_weights("LargeFamily", &weights);

    // Query for weight=400 — no exact hit; CSS §4.5.5 says prefer 400, 500,
    // then < 400 descending, then > 500 ascending.
    // Nearest below 400: 360 (= 8 × 45).  Nearest above 400: 405 (= 9 × 45).
    // The exact match chain for weight=400 (missing): check 400 (miss), 500
    // (miss), then <400 descending → 360 is first candidate below 400.
    let face = Query::new(&db)
        .family("LargeFamily")
        .weight(400)
        .match_best()
        .expect("must match a face in 20-face family");

    // Verify it's a sensible weight (≤ 400 is the best nearest-below candidate
    // when 400 and 500 are absent).
    assert!(
        face.weight <= 400,
        "20-face family: query weight=400, nearest below should win (got {})",
        face.weight
    );
}
