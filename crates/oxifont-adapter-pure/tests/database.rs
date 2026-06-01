//! Integration tests for `oxifont-adapter-pure`.

use oxifont_adapter_pure::FontDatabase;
use oxifont_core::{FontCatalog as _, FontQuery};
use std::env;
use std::fs;

/// Path to the shared parser fixture.
static FIXTURE_BYTES: &[u8] = include_bytes!("../../oxifont-parser/tests/fixtures/test.ttf");

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create a unique temp directory for one test, write `test.ttf` into it, and
/// return the directory path (caller must clean up).
fn setup_tmp_dir(suffix: &str) -> std::path::PathBuf {
    let tmp = env::temp_dir().join(format!("oxifont_pure_{}_{}", std::process::id(), suffix,));
    fs::create_dir_all(&tmp).expect("failed to create temp dir");
    let font_path = tmp.join("test.ttf");
    fs::write(&font_path, FIXTURE_BYTES).expect("failed to write fixture");
    tmp
}

// ---------------------------------------------------------------------------
// Original tests (unchanged)
// ---------------------------------------------------------------------------

#[test]
fn scan_dir_with_fixture_yields_non_empty_catalog() {
    let tmp = env::temp_dir().join(format!("oxifont_test_{}", std::process::id()));
    fs::create_dir_all(&tmp).expect("failed to create temp dir");
    let font_path = tmp.join("test.ttf");
    fs::write(&font_path, FIXTURE_BYTES).expect("failed to write fixture");

    let db = FontDatabase::scan(&[&tmp]).expect("scan must not error");
    assert!(
        !db.faces().is_empty(),
        "scan of temp dir with one font must yield at least one face"
    );

    // Clean up.
    let _ = fs::remove_file(&font_path);
    let _ = fs::remove_dir(&tmp);
}

#[test]
fn find_by_family_substring() {
    let tmp = env::temp_dir().join(format!("oxifont_find_{}", std::process::id()));
    fs::create_dir_all(&tmp).expect("failed to create temp dir");
    let font_path = tmp.join("test.ttf");
    fs::write(&font_path, FIXTURE_BYTES).expect("failed to write fixture");

    let db = FontDatabase::scan(&[&tmp]).expect("scan must not error");
    let faces = db.faces();
    assert!(!faces.is_empty());

    // We know the exact family name from the face we just scanned.
    let known_family = faces[0].family.clone();
    // A substring (first 3 chars if long enough, else the whole name) must match.
    let needle = if known_family.len() >= 3 {
        known_family[..3].to_string()
    } else {
        known_family.to_string()
    };

    let found = db.find(&FontQuery::new().family(&needle));
    assert!(
        found.is_some(),
        "find with partial family {:?} must match {:?}",
        needle,
        known_family
    );

    // Clean up.
    let _ = fs::remove_file(&font_path);
    let _ = fs::remove_dir(&tmp);
}

#[test]
fn find_with_empty_query_returns_first_face() {
    let tmp = env::temp_dir().join(format!("oxifont_empty_{}", std::process::id()));
    fs::create_dir_all(&tmp).expect("failed to create temp dir");
    let font_path = tmp.join("test.ttf");
    fs::write(&font_path, FIXTURE_BYTES).expect("failed to write fixture");

    let db = FontDatabase::scan(&[&tmp]).expect("scan must not error");
    assert!(!db.faces().is_empty());

    let found = db.find(&FontQuery::new());
    assert!(found.is_some(), "empty query must return at least one face");

    // Clean up.
    let _ = fs::remove_file(&font_path);
    let _ = fs::remove_dir(&tmp);
}

#[test]
fn scan_empty_dir_yields_empty_catalog() {
    let tmp = env::temp_dir().join(format!("oxifont_empty_dir_{}", std::process::id()));
    fs::create_dir_all(&tmp).expect("failed to create temp dir");

    let db = FontDatabase::scan(&[&tmp]).expect("scan must not error");
    assert!(
        db.faces().is_empty(),
        "scan of empty dir must yield no faces"
    );

    let _ = fs::remove_dir(&tmp);
}

// ---------------------------------------------------------------------------
// New tests — task 10
// ---------------------------------------------------------------------------

/// Task 10a: `add_dir` loads faces from a directory.
#[test]
fn test_add_dir_loads_faces() {
    let tmp = setup_tmp_dir("add_dir");

    let mut db = FontDatabase::new();
    db.add_dir(&tmp);

    assert!(
        !db.is_empty(),
        "add_dir on a dir containing test.ttf must yield at least one face"
    );

    // Clean up.
    let _ = fs::remove_file(tmp.join("test.ttf"));
    let _ = fs::remove_dir(&tmp);
}

/// Task 10b: `add_bytes` parses bytes and the family shows up in `find_all`.
#[test]
fn test_add_bytes_loads_face() {
    let mut db = FontDatabase::new();
    let n = db
        .add_bytes(FIXTURE_BYTES.to_vec(), None)
        .expect("add_bytes must succeed for valid TTF");
    assert!(n > 0, "at least one face must have been added");

    // Discover the family name from the face that was just added.
    let family = db.faces()[0].family.clone();

    let hits = db.find_all(&family);
    assert!(
        !hits.is_empty(),
        "find_all({:?}) must return the added face",
        family
    );
}

/// Task 10c: `remove` reduces the face count.
#[test]
fn test_remove_reduces_count() {
    let tmp = setup_tmp_dir("remove");
    let font_path = tmp.join("test.ttf");

    let mut db = FontDatabase::new();
    db.add_dir(&tmp);
    let before = db.len();
    assert!(
        !db.is_empty(),
        "precondition: at least one face must be present"
    );

    let removed = db.remove(&font_path);
    assert!(removed > 0, "remove must return a positive count");
    assert_eq!(
        db.len(),
        before - removed,
        "len must decrease by the number of removed faces"
    );

    // Clean up.
    let _ = fs::remove_file(&font_path);
    let _ = fs::remove_dir(&tmp);
}

/// Task 10d: `find_all` is case-insensitive (exact full-name match).
#[test]
fn test_find_all_case_insensitive() {
    let mut db = FontDatabase::new();
    db.add_bytes(FIXTURE_BYTES.to_vec(), None)
        .expect("add_bytes must succeed");

    let family = db.faces()[0].family.clone();

    // Upper-case version must still resolve.
    let upper = family.to_uppercase();
    let hits = db.find_all(&upper);
    assert!(
        !hits.is_empty(),
        "find_all must match {:?} against stored family {:?}",
        upper,
        family
    );

    // Mixed-case version must also resolve.
    let mixed: String = family
        .chars()
        .enumerate()
        .map(|(i, c)| {
            if i % 2 == 0 {
                c.to_uppercase().next().unwrap_or(c)
            } else {
                c.to_lowercase().next().unwrap_or(c)
            }
        })
        .collect();
    let hits2 = db.find_all(&mixed);
    assert!(
        !hits2.is_empty(),
        "find_all must match mixed-case {:?} against stored family {:?}",
        mixed,
        family
    );
}

/// Task 10e: `for face in &db` syntax compiles and iterates all faces.
#[test]
fn test_into_iterator() {
    let mut db = FontDatabase::new();
    db.add_bytes(FIXTURE_BYTES.to_vec(), None)
        .expect("add_bytes must succeed");

    let mut count = 0usize;
    for _face in &db {
        count += 1;
    }
    assert_eq!(
        count,
        db.len(),
        "iterator must visit every face exactly once"
    );
}

/// Task 10f: `merge` combines two databases.
#[test]
fn test_merge_combines() {
    let mut db1 = FontDatabase::new();
    db1.add_bytes(FIXTURE_BYTES.to_vec(), None)
        .expect("add_bytes must succeed");
    let count1 = db1.len();

    let mut db2 = FontDatabase::new();
    db2.add_bytes(FIXTURE_BYTES.to_vec(), None)
        .expect("add_bytes must succeed");
    let count2 = db2.len();

    db1.merge(db2);
    assert_eq!(
        db1.len(),
        count1 + count2,
        "merged database must contain the sum of both databases"
    );
}

/// Task 10g: `len` and `is_empty` report the correct state.
#[test]
fn test_len_is_empty() {
    let mut db = FontDatabase::new();
    assert!(db.is_empty(), "new database must be empty");
    assert_eq!(db.len(), 0, "new database must have len 0");

    db.add_bytes(FIXTURE_BYTES.to_vec(), None)
        .expect("add_bytes must succeed");
    assert!(!db.is_empty(), "database must not be empty after add_bytes");
    assert!(
        !db.is_empty(),
        "database must not be empty (len) after add_bytes"
    );
}
