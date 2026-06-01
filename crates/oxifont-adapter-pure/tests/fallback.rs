//! Tests for `FontDatabase::find_with_fallback`.

use oxifont_adapter_pure::FontDatabase;
use oxifont_core::{FaceInfo, FontQuery, FontStretch, FontStyle};
use std::path::PathBuf;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

fn make_face(family: &str, weight: u16) -> FaceInfo {
    FaceInfo {
        family: Arc::from(family),
        post_script_name: String::new(),
        style: FontStyle::Normal,
        weight,
        stretch: FontStretch::Normal,
        path: PathBuf::from("/dev/null"),
        face_index: 0,
        localized_families: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// The first family in the list that is present in the database is returned.
#[test]
fn test_find_with_fallback_finds_first_match() {
    let db = FontDatabase::from_faces(vec![make_face("Arial", 400), make_face("Helvetica", 400)]);

    let base = FontQuery::new().weight(400);
    let result = db.find_with_fallback(&["Arial", "Helvetica"], &base, "Hello");
    assert!(result.is_some(), "should find Arial");
    assert_eq!(&*result.unwrap().family, "Arial");
}

/// When the first family is absent, the search continues to the next.
#[test]
fn test_find_with_fallback_falls_to_second_family() {
    let db = FontDatabase::from_faces(vec![make_face("Helvetica", 400)]);

    let base = FontQuery::new().weight(400);
    // "Arial" is not in the db; "Helvetica" is.
    let result = db.find_with_fallback(&["Arial", "Helvetica"], &base, "Hello");
    assert!(result.is_some(), "should fall back to Helvetica");
    assert_eq!(&*result.unwrap().family, "Helvetica");
}

/// Returns `None` when none of the requested families exist.
#[test]
fn test_find_with_fallback_returns_none_when_no_match() {
    let db = FontDatabase::from_faces(vec![make_face("Courier New", 400)]);

    let base = FontQuery::new();
    let result = db.find_with_fallback(&["NonExistentFont", "AlsoMissing"], &base, "Hello");
    assert!(
        result.is_none(),
        "should return None when no family matches"
    );
}

/// An empty families slice always returns `None`.
#[test]
fn test_find_with_fallback_empty_families_slice() {
    let db = FontDatabase::from_faces(vec![make_face("Arial", 400)]);

    let base = FontQuery::new();
    let result = db.find_with_fallback(&[], &base, "Hello");
    assert!(result.is_none(), "empty families slice must return None");
}

/// CSS generic families (e.g. `"sans-serif"`) are resolved via the generic
/// alias table and count as valid candidates.
#[test]
fn test_find_with_fallback_resolves_generic_family() {
    // Arial is the first concrete family for "sans-serif" in the alias table.
    let db = FontDatabase::from_faces(vec![make_face("Arial", 400)]);

    let base = FontQuery::new();
    // The exact name "NonExistent" is absent, but "sans-serif" resolves to Arial.
    let result = db.find_with_fallback(&["NonExistent", "sans-serif"], &base, "Hello");
    assert!(result.is_some(), "sans-serif should resolve to Arial");
    assert_eq!(&*result.unwrap().family, "Arial");
}

/// Weight and style constraints from `base_query` are forwarded to each
/// per-family CSS query, so only faces matching those constraints are returned.
#[test]
fn test_find_with_fallback_applies_weight_constraint() {
    let db = FontDatabase::from_faces(vec![
        make_face("Arial", 400),
        make_face("Arial", 700),
        make_face("Helvetica", 700),
    ]);

    // Ask for weight=700 specifically.
    let base = FontQuery::new().weight(700);
    let result = db.find_with_fallback(&["Arial", "Helvetica"], &base, "Hello");
    assert!(result.is_some());
    // CSS weight narrowing picks the 700-weight face for Arial.
    assert_eq!(result.unwrap().weight, 700);
}

/// The `text` parameter is accepted without error; the function does not panic
/// on multi-byte Unicode content.
#[test]
fn test_find_with_fallback_accepts_unicode_text() {
    let db = FontDatabase::from_faces(vec![make_face("Noto Sans", 400)]);

    let base = FontQuery::new();
    let result = db.find_with_fallback(&["Noto Sans"], &base, "こんにちは 🎉");
    assert!(result.is_some());
}
