//! Integration tests for the `oxifont-db` bridge (`db` Cargo feature):
//! `NativeCatalog::into_db()` and `NativeCatalog::as_db()`.
//!
//! The CoreText test runs on macOS hosts (this crate's primary CI/dev
//! platform). The DirectWrite test is `#[cfg(windows)]`-gated like the rest
//! of `tests/directwrite.rs` and can only run on Windows. A placeholder test
//! keeps the binary non-empty everywhere else.

#[cfg(all(target_os = "macos", feature = "db"))]
#[test]
fn coretext_into_db_is_queryable_by_css_level_4() {
    use oxifont_adapter_native::NativeCatalog;
    use oxifont_core::FontCatalog as _;
    use oxifont_db::Query;

    let native = NativeCatalog::load().expect("CoreText catalog load failed");
    let native_face_count = native.faces().len();

    let db = native.into_db();
    assert_eq!(
        db.stats().face_count,
        native_face_count,
        "into_db() must carry over every face"
    );

    // At least one of these generic families must resolve on any standard
    // macOS installation once the native faces are indexed for CSS querying.
    let matched = Query::new(&db)
        .family("Helvetica")
        .match_best()
        .or_else(|| Query::new(&db).family("Menlo").match_best())
        .or_else(|| Query::new(&db).family("Arial").match_best());
    assert!(
        matched.is_some(),
        "CSS query must find at least one of Helvetica/Menlo/Arial after into_db()"
    );
}

#[cfg(all(target_os = "macos", feature = "db"))]
#[test]
fn coretext_as_db_leaves_original_catalog_usable() {
    use oxifont_adapter_native::NativeCatalog;
    use oxifont_core::FontCatalog as _;

    let native = NativeCatalog::load().expect("CoreText catalog load failed");
    let db = native.as_db();
    assert_eq!(
        db.stats().face_count,
        native.faces().len(),
        "as_db() must carry over every face without consuming the catalog"
    );
    // `native` must still be usable after `as_db()` (it borrowed, not moved).
    assert!(!native.faces().is_empty());
}

#[cfg(all(windows, feature = "db"))]
#[test]
fn directwrite_into_db_is_queryable_by_css_level_4() {
    use oxifont_adapter_native::NativeCatalog;
    use oxifont_core::FontCatalog as _;
    use oxifont_db::Query;

    let native = NativeCatalog::load().expect("DirectWrite catalog load must succeed on Windows");
    let native_face_count = native.faces().len();

    let db = native.into_db();
    assert_eq!(
        db.stats().face_count,
        native_face_count,
        "into_db() must carry over every face"
    );

    let matched = Query::new(&db)
        .family("Segoe UI")
        .match_best()
        .or_else(|| Query::new(&db).family("Arial").match_best());
    assert!(
        matched.is_some(),
        "CSS query must find at least one of Segoe UI/Arial after into_db()"
    );
}

// ---------------------------------------------------------------------------
// Placeholder — ensures the test binary is never empty on platforms/feature
// combinations where none of the tests above are compiled in (e.g. the
// `db` feature disabled, or a non-macOS/non-Windows host).
// ---------------------------------------------------------------------------

#[cfg(not(all(any(target_os = "macos", windows), feature = "db")))]
#[test]
fn db_bridge_tests_require_macos_or_windows_with_db_feature() {
    // Real coverage lives in the macOS/Windows tests above.
}
