//! Tests for lazy font loading via metadata-only SFNT scanning.

use oxifont_adapter_pure::FontDatabase;
use oxifont_core::FontCatalog as _;
use std::path::PathBuf;

/// Shared fixture bytes embedded at compile time.
static FIXTURE_BYTES: &[u8] = include_bytes!("../../oxifont-parser/tests/fixtures/test.ttf");

/// Fixture directory path relative to the adapter-pure crate.
fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../oxifont-parser/tests/fixtures")
}

/// Create a unique temp directory with a TTF file; return the directory path.
fn setup_tmp_dir(suffix: &str) -> PathBuf {
    let tmp = std::env::temp_dir().join(format!("oxifont_lazy_{}_{}", std::process::id(), suffix));
    std::fs::create_dir_all(&tmp).expect("failed to create temp dir");
    std::fs::write(tmp.join("test.ttf"), FIXTURE_BYTES).expect("failed to write fixture");
    tmp
}

#[test]
fn system_lazy_returns_database() {
    let result = FontDatabase::system_lazy();
    assert!(
        result.is_ok(),
        "system_lazy() must not return an error: {:?}",
        result.err()
    );
}

#[test]
fn scan_lazy_with_fixture_dir() {
    let dir = fixture_dir();
    let db = FontDatabase::scan_lazy(&[dir]).expect("scan_lazy must succeed on fixture dir");
    assert!(
        !db.faces().is_empty(),
        "fixture dir must have at least one face"
    );
    let info = &db.faces()[0];
    assert!(!info.family.is_empty(), "family must not be empty");
}

#[test]
fn scan_lazy_with_tmp_dir() {
    let tmp = setup_tmp_dir("scan_lazy");
    let db = FontDatabase::scan_lazy(std::slice::from_ref(&tmp)).expect("scan_lazy must succeed");
    assert!(
        !db.faces().is_empty(),
        "tmp dir must have at least one face"
    );
    let info = &db.faces()[0];
    assert!(!info.family.is_empty(), "family must not be empty");
    assert_eq!(info.face_index, 0, "TTF always has face_index 0");
    assert_eq!(
        info.path,
        tmp.join("test.ttf"),
        "path must point to fixture"
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn lazy_metadata_matches_eager_metadata() {
    let tmp = setup_tmp_dir("meta_cmp");

    let lazy_db = FontDatabase::scan_lazy(std::slice::from_ref(&tmp)).expect("scan_lazy");
    let eager_db = FontDatabase::scan(std::slice::from_ref(&tmp)).expect("scan (eager)");

    assert_eq!(
        lazy_db.faces().len(),
        eager_db.faces().len(),
        "face counts must match"
    );

    if let (Some(lazy_face), Some(eager_face)) = (lazy_db.faces().first(), eager_db.faces().first())
    {
        assert_eq!(lazy_face.family, eager_face.family, "families must match");
        assert_eq!(lazy_face.weight, eager_face.weight, "weights must match");
        assert_eq!(lazy_face.style, eager_face.style, "styles must match");
        assert_eq!(
            lazy_face.stretch, eager_face.stretch,
            "stretches must match"
        );
        assert_eq!(
            lazy_face.post_script_name, eager_face.post_script_name,
            "postscript names must match"
        );
    }

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn load_face_after_lazy_scan() {
    let tmp = setup_tmp_dir("load_face");

    let db = FontDatabase::scan_lazy(std::slice::from_ref(&tmp)).expect("scan_lazy");
    assert!(!db.faces().is_empty(), "must have at least one face");

    let info = &db.faces()[0];
    let parsed = db.load_face(info).expect("load_face must succeed");

    // Verify the parsed face has matching family.
    use oxifont_core::FontFace as _;
    assert_eq!(
        parsed.family_name(),
        info.family.as_ref(),
        "loaded face family must match FaceInfo"
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn scan_lazy_empty_dir_returns_empty_db() {
    let tmp = std::env::temp_dir().join(format!(
        "oxifont_lazy_empty_{}_{}",
        std::process::id(),
        "empty"
    ));
    std::fs::create_dir_all(&tmp).expect("failed to create temp dir");

    let db = FontDatabase::scan_lazy(std::slice::from_ref(&tmp)).expect("scan_lazy on empty dir");
    assert!(db.faces().is_empty(), "empty dir must yield empty catalog");

    let _ = std::fs::remove_dir_all(&tmp);
}
