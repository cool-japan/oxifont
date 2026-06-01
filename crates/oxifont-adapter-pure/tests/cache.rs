//! Integration tests for the disk-caching layer (`feature = "cache"`).
//!
//! Each test uses an isolated temp directory and sets `OXIFONT_CACHE_DIR` to
//! a per-test directory so the tests are hermetic and do not touch the
//! user's real cache.

#![cfg(feature = "cache")]

use oxifont_adapter_pure::FontDatabase;
use oxifont_core::FontCatalog as _;
use std::env;
use std::fs;

/// Bytes of the shared parser fixture (a minimal valid TTF).
static FIXTURE_BYTES: &[u8] = include_bytes!("../../oxifont-parser/tests/fixtures/test.ttf");

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create a temp directory and write `test.ttf` into it.  Returns the
/// directory path; the caller is responsible for cleanup.
fn setup_font_dir(label: &str) -> std::path::PathBuf {
    let tmp = env::temp_dir().join(format!(
        "oxifont_cache_test_{}_{}",
        std::process::id(),
        label,
    ));
    fs::create_dir_all(&tmp).expect("create font dir");
    fs::write(tmp.join("test.ttf"), FIXTURE_BYTES).expect("write fixture");
    tmp
}

/// Create a temp directory for the cache.  Returns the directory path.
fn setup_cache_dir(label: &str) -> std::path::PathBuf {
    let tmp = env::temp_dir().join(format!(
        "oxifont_cache_dir_{}_{}",
        std::process::id(),
        label,
    ));
    fs::create_dir_all(&tmp).expect("create cache dir");
    tmp
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// `scan_cached` must populate the catalog from the font directory.
#[test]
fn test_scan_cached_populates_catalog() {
    let font_dir = setup_font_dir("populate");
    let cache_dir = setup_cache_dir("populate");

    // Point OXIFONT_CACHE_DIR at our temp dir so we don't pollute the user's
    // real cache, and so parallel tests don't collide.
    env::set_var("OXIFONT_CACHE_DIR", &cache_dir);

    let db = FontDatabase::scan_cached(&[&font_dir]).expect("scan_cached must not error");
    assert!(!db.faces().is_empty(), "catalog must be non-empty");

    // Clean up.
    env::remove_var("OXIFONT_CACHE_DIR");
    let _ = fs::remove_file(font_dir.join("test.ttf"));
    let _ = fs::remove_dir(&font_dir);
    let _ = fs::remove_dir_all(&cache_dir);
}

/// A second call with the same font directory and cache must produce the same
/// result (cache hit path).
#[test]
fn test_scan_cached_cache_hit_consistent() {
    let font_dir = setup_font_dir("hit");
    let cache_dir = setup_cache_dir("hit");
    env::set_var("OXIFONT_CACHE_DIR", &cache_dir);

    // First scan: cold start, fills the cache.
    let db1 = FontDatabase::scan_cached(&[&font_dir]).expect("first scan must succeed");
    let count1 = db1.faces().len();
    assert!(count1 > 0, "first scan must yield at least one face");

    // Second scan: cache hit; result must match.
    let db2 = FontDatabase::scan_cached(&[&font_dir]).expect("second scan must succeed");
    let count2 = db2.faces().len();
    assert_eq!(
        count1, count2,
        "cache hit must yield the same number of faces as the first scan"
    );

    // Family names must match too.
    let families1: Vec<&str> = db1.faces().iter().map(|f| &*f.family).collect();
    let families2: Vec<&str> = db2.faces().iter().map(|f| &*f.family).collect();
    assert_eq!(
        families1, families2,
        "cache hit must yield identical family names"
    );

    env::remove_var("OXIFONT_CACHE_DIR");
    let _ = fs::remove_file(font_dir.join("test.ttf"));
    let _ = fs::remove_dir(&font_dir);
    let _ = fs::remove_dir_all(&cache_dir);
}

/// Removing a font file and scanning again must prune the stale cache entry
/// and return a smaller catalog.
#[test]
fn test_scan_cached_prunes_stale_entry() {
    let font_dir = setup_font_dir("prune");
    let cache_dir = setup_cache_dir("prune");
    env::set_var("OXIFONT_CACHE_DIR", &cache_dir);

    // First scan: cache the one font.
    let db1 = FontDatabase::scan_cached(&[&font_dir]).expect("first scan");
    let count1 = db1.faces().len();
    assert!(count1 > 0);

    // Remove the font file.
    fs::remove_file(font_dir.join("test.ttf")).expect("remove font file");

    // Second scan: the font is gone; catalog must be empty.
    let db2 = FontDatabase::scan_cached(&[&font_dir]).expect("second scan");
    assert_eq!(
        db2.faces().len(),
        0,
        "catalog must be empty after font file is removed"
    );

    env::remove_var("OXIFONT_CACHE_DIR");
    let _ = fs::remove_dir(&font_dir);
    let _ = fs::remove_dir_all(&cache_dir);
}

/// `scan_cached` on an empty directory must return an empty catalog without
/// error — same contract as `scan`.
#[test]
fn test_scan_cached_empty_dir_yields_empty_catalog() {
    let empty_dir = env::temp_dir().join(format!(
        "oxifont_cache_empty_{}_{}",
        std::process::id(),
        "empty",
    ));
    fs::create_dir_all(&empty_dir).expect("create empty dir");
    let cache_dir = setup_cache_dir("empty");
    env::set_var("OXIFONT_CACHE_DIR", &cache_dir);

    let db = FontDatabase::scan_cached(&[&empty_dir]).expect("scan_cached must not error");
    assert!(
        db.is_empty(),
        "scan_cached of empty dir must yield empty catalog"
    );

    env::remove_var("OXIFONT_CACHE_DIR");
    let _ = fs::remove_dir(&empty_dir);
    let _ = fs::remove_dir_all(&cache_dir);
}
