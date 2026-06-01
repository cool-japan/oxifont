//! Integration tests for new API items added in oxifont-db.
//!
//! Covers:
//! - `Display for FaceInfo`
//! - `FontDatabase::stats()`
//! - `FontDatabase::remove_face()`
//! - `FontDatabase::sort_family_index()`

use oxifont_db::{FaceInfo, FontDatabase, Source};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_face(family: &str, weight: u16, italic: bool) -> FaceInfo {
    FaceInfo {
        id: 0,
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

fn make_file_face(family: &str, weight: u16, path: &str, face_index: u32) -> FaceInfo {
    FaceInfo {
        id: 0,
        family: family.to_string(),
        post_script_name: String::new(),
        weight,
        italic: false,
        stretch: 5,
        monospaced: false,
        source: Source::File(std::path::PathBuf::from(path)),
        face_index,
        variable_axes: Vec::new(),
        locale_families: Vec::new(),
        unicode_ranges: 0,
    }
}

fn make_face_with_psn(family: &str, weight: u16, psn: &str) -> FaceInfo {
    FaceInfo {
        post_script_name: psn.to_string(),
        ..make_face(family, weight, false)
    }
}

// ---------------------------------------------------------------------------
// Display for FaceInfo
// ---------------------------------------------------------------------------

#[test]
fn test_display_face_info_memory_source() {
    let face = make_face("Roboto", 700, false);
    let s = face.to_string();
    assert!(s.contains("Roboto"), "should contain family name");
    assert!(s.contains("700"), "should contain weight");
    assert!(s.contains("Bold"), "weight 700 → Bold style name");
    assert!(s.contains("memory"), "memory source should show <memory>");
}

#[test]
fn test_display_face_info_italic() {
    let face = make_face("Open Sans", 400, true);
    let s = face.to_string();
    assert!(s.contains("Open Sans"), "family name");
    assert!(
        s.contains("Italic"),
        "italic face should have Italic in style"
    );
}

#[test]
fn test_display_face_info_file_source_no_index() {
    let face = make_file_face("Helvetica", 400, "/System/Library/Fonts/Helvetica.ttf", 0);
    let s = face.to_string();
    assert!(s.contains("Helvetica"), "family");
    assert!(s.contains("Helvetica.ttf"), "path");
    // face_index == 0 → no bracket suffix
    assert!(
        !s.contains('['),
        "face_index 0 should not produce bracket suffix"
    );
}

#[test]
fn test_display_face_info_file_source_with_index() {
    let face = make_file_face("Helvetica", 700, "/System/Library/Fonts/Helvetica.ttc", 1);
    let s = face.to_string();
    assert!(
        s.contains("Helvetica.ttc[1]"),
        "TTC face_index > 0 must show bracket index"
    );
}

// Weight edge cases
#[test]
fn test_display_weight_names() {
    let cases: &[(u16, &str)] = &[
        (100, "Thin"),
        (200, "ExtraLight"),
        (300, "Light"),
        (400, "Regular"),
        (500, "Medium"),
        (600, "SemiBold"),
        (700, "Bold"),
        (800, "ExtraBold"),
        (900, "Black"),
    ];
    for &(weight, expected_style) in cases {
        let face = make_face("TestFamily", weight, false);
        let s = face.to_string();
        assert!(
            s.contains(expected_style),
            "weight {weight} → expected style '{expected_style}' in '{s}'"
        );
    }
}

// ---------------------------------------------------------------------------
// FontDatabase::stats()
// ---------------------------------------------------------------------------

#[test]
fn test_stats_empty_database() {
    let db = FontDatabase::new();
    let stats = db.stats();
    assert_eq!(stats.face_count, 0);
    assert_eq!(stats.family_count, 0);
    assert!(stats.cache_path.is_none());
}

#[test]
fn test_stats_face_and_family_count() {
    // 3 faces in 2 families.
    let mut db = FontDatabase::new();
    db.add_face(make_face("FamilyA", 400, false));
    db.add_face(make_face("FamilyA", 700, false));
    db.add_face(make_face("FamilyB", 400, false));

    let stats = db.stats();
    assert_eq!(stats.face_count, 3, "three faces");
    assert_eq!(stats.family_count, 2, "two families");
    assert!(stats.cache_path.is_none(), "no cache path for fresh db");
}

#[test]
fn test_stats_family_count_case_insensitive() {
    // "Arial" and "arial" should be the same family.
    let mut db = FontDatabase::new();
    db.add_face(make_face("Arial", 400, false));
    db.add_face(make_face("arial", 700, false));

    let stats = db.stats();
    assert_eq!(stats.face_count, 2);
    assert_eq!(stats.family_count, 1, "case-insensitive family fold");
}

// ---------------------------------------------------------------------------
// FontDatabase::remove_face()
// ---------------------------------------------------------------------------

#[test]
fn test_remove_face_unknown_id_returns_false() {
    let mut db = FontDatabase::new();
    db.add_face(make_face("Alpha", 400, false));
    assert!(
        !db.remove_face(999),
        "removing non-existent ID should return false"
    );
    assert_eq!(db.faces().len(), 1, "database unchanged");
}

#[test]
fn test_remove_face_basic() {
    let mut db = FontDatabase::new();
    let idx = db.add_face(make_face("Alpha", 400, false));
    let id = db.faces()[idx].id;

    assert!(db.remove_face(id), "should return true for known ID");
    assert_eq!(db.faces().len(), 0, "face removed from flat store");
    assert!(db.face_by_id(id).is_none(), "face_by_id should return None");
}

#[test]
fn test_remove_face_decreases_count_and_unfindable() {
    let mut db = FontDatabase::new();
    db.add_face(make_face("FamilyA", 400, false));
    let idx = db.add_face(make_face("FamilyA", 700, false));
    db.add_face(make_face("FamilyB", 400, false));
    let remove_id = db.faces()[idx].id;

    assert!(db.remove_face(remove_id));
    assert_eq!(db.faces().len(), 2, "one face removed");

    // The removed face must not appear in by-family lookup.
    let family_a = db.faces_by_family("FamilyA");
    assert_eq!(family_a.len(), 1, "FamilyA now has 1 face");
    assert!(
        family_a.iter().all(|f| f.id != remove_id),
        "removed face must not be in family index"
    );
}

#[test]
fn test_remove_face_cleans_empty_family() {
    let mut db = FontDatabase::new();
    let idx = db.add_face(make_face("OnlyChild", 400, false));
    let id = db.faces()[idx].id;

    assert!(db.remove_face(id));
    // Family "onlychild" should be gone entirely.
    let family = db.faces_by_family("OnlyChild");
    assert!(
        family.is_empty(),
        "family entry must be reclaimed when empty"
    );
    let stats = db.stats();
    assert_eq!(stats.family_count, 0);
}

#[test]
fn test_remove_face_removes_postscript_index() {
    let mut db = FontDatabase::new();
    let idx = db.add_face(make_face_with_psn("Alpha", 400, "Alpha-Regular"));
    let id = db.faces()[idx].id;

    assert!(db.find_by_postscript_name("Alpha-Regular").is_some());
    assert!(db.remove_face(id));
    assert!(
        db.find_by_postscript_name("Alpha-Regular").is_none(),
        "PostScript name index must be cleaned up"
    );
}

#[test]
fn test_remove_face_remaining_ids_still_findable() {
    // After removing a face in the middle, remaining faces must still be
    // findable by their original IDs.
    let mut db = FontDatabase::new();
    let i0 = db.add_face(make_face("F", 100, false));
    let i1 = db.add_face(make_face("F", 400, false));
    let i2 = db.add_face(make_face("F", 700, false));

    let id0 = db.faces()[i0].id;
    let id1 = db.faces()[i1].id;
    let id2 = db.faces()[i2].id;

    // Remove the middle one.
    assert!(db.remove_face(id1));
    assert_eq!(db.faces().len(), 2);

    // The other two must still be findable.
    assert!(db.face_by_id(id0).is_some(), "id0 must still resolve");
    assert!(db.face_by_id(id2).is_some(), "id2 must still resolve");
    assert!(db.face_by_id(id1).is_none(), "id1 must be gone");
}

// ---------------------------------------------------------------------------
// FontDatabase::sort_family_index()
// ---------------------------------------------------------------------------

#[test]
fn test_sort_within_family_by_weight() {
    let mut db = FontDatabase::new();
    // Insert in reverse weight order.
    db.add_face(make_face("SortTest", 900, false));
    db.add_face(make_face("SortTest", 100, false));
    db.add_face(make_face("SortTest", 400, false));

    db.sort_family_index();

    let faces = db.faces_by_family("SortTest");
    assert_eq!(faces.len(), 3);
    let weights: Vec<u16> = faces.iter().map(|f| f.weight).collect();
    assert_eq!(
        weights,
        vec![100, 400, 900],
        "faces must be sorted by weight ascending"
    );
}

#[test]
fn test_sort_family_index_multiple_families() {
    let mut db = FontDatabase::new();
    db.add_face(make_face("AAA", 700, false));
    db.add_face(make_face("BBB", 900, false));
    db.add_face(make_face("AAA", 100, false));
    db.add_face(make_face("BBB", 300, false));

    db.sort_family_index();

    let aaa = db.faces_by_family("AAA");
    assert_eq!(aaa[0].weight, 100);
    assert_eq!(aaa[1].weight, 700);

    let bbb = db.faces_by_family("BBB");
    assert_eq!(bbb[0].weight, 300);
    assert_eq!(bbb[1].weight, 900);
}

// ---------------------------------------------------------------------------
// FontDatabase::find_by_postscript_name() — O(1) HashMap index
// ---------------------------------------------------------------------------

#[test]
fn test_postscript_name_index_o1_lookup() {
    let mut db = FontDatabase::new();
    db.add_face(make_face_with_psn("TestFamily", 400, "TestFont-Regular"));
    db.add_face(make_face_with_psn("TestFamily", 700, "TestFont-Bold"));
    db.add_face(make_face("NoName", 400, false)); // empty post_script_name

    let found = db.find_by_postscript_name("TestFont-Regular");
    assert!(found.is_some(), "exact PostScript name should resolve");
    assert_eq!(found.unwrap().weight, 400);

    let bold = db.find_by_postscript_name("TestFont-Bold");
    assert!(bold.is_some());
    assert_eq!(bold.unwrap().weight, 700);

    assert!(
        db.find_by_postscript_name("NonExistent").is_none(),
        "missing PostScript name should return None"
    );
}

#[test]
fn test_postscript_name_lookup_case_sensitive() {
    let mut db = FontDatabase::new();
    db.add_face(make_face_with_psn("X", 400, "ExactCase-Regular"));

    // PostScript name lookups are case-sensitive.
    assert!(db.find_by_postscript_name("ExactCase-Regular").is_some());
    assert!(
        db.find_by_postscript_name("exactcase-regular").is_none(),
        "lookup must be case-sensitive"
    );
}

// ---------------------------------------------------------------------------
// FontDatabase::load_system_fonts_bg() — thread-based background loading
// ---------------------------------------------------------------------------

/// System font scan can be slow on CI; mark ignored by default.
/// Run with `cargo test -- --ignored test_load_system_fonts_bg` to exercise.
#[test]
#[ignore]
fn test_load_system_fonts_bg_returns_handle() {
    let handle = FontDatabase::load_system_fonts_bg();
    // May return an empty DB on CI, but must not panic.
    let db = handle.join().expect("background load must not panic");
    // stats() must always succeed regardless of whether any fonts were found.
    let stats = db.stats();
    let _ = stats.face_count;
}
