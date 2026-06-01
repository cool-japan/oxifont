//! Integration tests for `oxifont-discovery`.

#[test]
fn test_system_font_dirs_nonempty() {
    let dirs = oxifont_discovery::system_font_dirs();
    // At least one directory must be returned on any supported platform.
    assert!(!dirs.is_empty());
}

#[test]
fn test_scan_with_mtime_returns_faces() {
    let dirs = oxifont_discovery::system_font_dirs();
    let results = oxifont_discovery::scan_dirs_with_mtime(&dirs);
    // On a system with fonts we get results; on a bare CI image we may get
    // zero — the key requirement is no panic.
    let _ = results.len();
}

#[test]
fn test_max_mtime_of_dirs_does_not_panic() {
    let dirs = oxifont_discovery::system_font_dirs();
    let mtime = oxifont_discovery::max_mtime_of_dirs(&dirs);
    // Must return something, even UNIX_EPOCH for empty / mtime-less dirs.
    let _ = mtime;
}

#[test]
fn test_scan_with_progress_calls_callback() {
    let dirs = oxifont_discovery::system_font_dirs();
    let mut call_count = 0usize;
    let _faces = oxifont_discovery::scan_dirs_with_progress(&dirs, |_faces_so_far, _path| {
        call_count += 1;
    });
    // Progress callback must be called at least once if dirs are non-empty.
    // Do not assert > 0 because CI may have empty dirs.
    let _ = call_count;
}

#[test]
fn test_bsd_dirs_compile() {
    // Ensures the FreeBSD / BSD cfg branch compiles. The function is
    // cfg-gated, so on other platforms this just exercises the same
    // system_font_dirs() call path without a BSD-specific assertion.
    let _ = oxifont_discovery::system_font_dirs();
}

#[test]
fn test_mtime_face_fields_accessible() {
    // Construct a minimal temp dir with a real TTF to verify that
    // FaceWithMtime exposes `face`, `mtime`, and `path` correctly.
    static TTF: &[u8] = include_bytes!("../../oxifont-parser/tests/fixtures/test.ttf");

    let tmp = std::env::temp_dir().join("oxifont_disco_mtime_test");
    let _ = std::fs::create_dir_all(&tmp);
    let font_path = tmp.join("face.ttf");
    std::fs::write(&font_path, TTF).expect("write temp TTF");

    let results = oxifont_discovery::scan_dirs_with_mtime(&[&tmp]);
    assert_eq!(results.len(), 1, "one face expected from the fixture TTF");
    let item = &results[0];
    assert!(!item.face.family.is_empty(), "family must not be empty");
    assert_eq!(item.path, font_path, "path must match written file");
    // mtime should be at or after UNIX_EPOCH (just must not panic).
    let _ = item.mtime;

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn test_max_mtime_of_dirs_increases_with_new_file() {
    static TTF: &[u8] = include_bytes!("../../oxifont-parser/tests/fixtures/test.ttf");

    let tmp = std::env::temp_dir().join("oxifont_disco_max_mtime_test");
    let _ = std::fs::create_dir_all(&tmp);

    // With no font files the mtime should be UNIX_EPOCH.
    let mtime_empty = oxifont_discovery::max_mtime_of_dirs(&[&tmp]);
    assert_eq!(
        mtime_empty,
        std::time::SystemTime::UNIX_EPOCH,
        "empty dir must yield UNIX_EPOCH"
    );

    // Write a font file; now mtime must be > UNIX_EPOCH.
    let font_path = tmp.join("max.ttf");
    std::fs::write(&font_path, TTF).expect("write temp TTF");
    let mtime_with_file = oxifont_discovery::max_mtime_of_dirs(&[&tmp]);
    assert!(
        mtime_with_file > std::time::SystemTime::UNIX_EPOCH,
        "mtime must advance once a font file is present"
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

// ---------------------------------------------------------------------------
// Background scanning API
// ---------------------------------------------------------------------------

#[test]
fn scan_dirs_background_completes_without_panic() {
    let tmp = std::env::temp_dir().join(format!("oxifont_bg_test_{}", std::process::id()));
    std::fs::create_dir_all(&tmp).expect("create dir");

    let handle = oxifont_discovery::scan_dirs_background(vec![tmp.clone()]);
    let result = handle.join().expect("background scan thread panicked");

    // Empty directory → 0 faces, no panic.
    assert_eq!(result.faces.len(), 0);

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn scan_dirs_background_finds_font_in_dir() {
    static TTF: &[u8] = include_bytes!("../../oxifont-parser/tests/fixtures/test.ttf");

    let tmp = std::env::temp_dir().join(format!("oxifont_bg_font_test_{}", std::process::id()));
    std::fs::create_dir_all(&tmp).expect("create dir");
    std::fs::write(tmp.join("font.ttf"), TTF).expect("write fixture TTF");

    let handle = oxifont_discovery::scan_dirs_background(vec![tmp.clone()]);
    let result = handle.join().expect("background scan thread panicked");

    assert_eq!(
        result.faces.len(),
        1,
        "one face expected from the fixture TTF"
    );
    assert_eq!(result.files_scanned, 1);
    assert!(result.errors.is_empty());

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn scan_system_fonts_background_returns_handle() {
    // Just verify it starts without panic; don't wait for it to finish in CI
    // (system scans can take seconds on macOS).
    #[cfg(target_os = "macos")]
    {
        let handle = oxifont_discovery::scan_system_fonts_background();
        let result = handle.join().expect("system scan panicked");
        // On macOS there should be at least some fonts.
        let _ = result;
    }
    // On other platforms: test with an empty dir to avoid long system scan.
    #[cfg(not(target_os = "macos"))]
    {
        let tmp = std::env::temp_dir().join(format!("oxifont_sysfonts_bg_{}", std::process::id()));
        std::fs::create_dir_all(&tmp).ok();
        let handle = oxifont_discovery::scan_dirs_background(vec![tmp.clone()]);
        let result = handle.join().expect("scan thread panicked");
        assert_eq!(result.faces.len(), 0);
        std::fs::remove_dir_all(&tmp).ok();
    }
}

#[test]
fn test_progress_callback_called_per_font_file() {
    static TTF: &[u8] = include_bytes!("../../oxifont-parser/tests/fixtures/test.ttf");

    let tmp = std::env::temp_dir().join("oxifont_disco_progress_test");
    let _ = std::fs::create_dir_all(&tmp);
    std::fs::write(tmp.join("a.ttf"), TTF).expect("write a.ttf");
    std::fs::write(tmp.join("b.ttf"), TTF).expect("write b.ttf");

    let mut call_count = 0usize;
    let faces = oxifont_discovery::scan_dirs_with_progress(&[&tmp], |_n, _p| {
        call_count += 1;
    });
    assert_eq!(call_count, 2, "callback must fire once per font file");
    assert_eq!(faces.len(), 2, "two valid TTFs must yield two faces");

    let _ = std::fs::remove_dir_all(&tmp);
}

// ---------------------------------------------------------------------------
// scan_dirs_metadata_only tests
// ---------------------------------------------------------------------------

static TTF_FIXTURE_META: &[u8] = include_bytes!("../../oxifont-parser/tests/fixtures/test.ttf");

#[test]
fn scan_dirs_metadata_only_returns_faces() {
    let fixture_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../oxifont-parser/tests/fixtures");
    let result = oxifont_discovery::scan_dirs_metadata_only(&[fixture_dir]);
    assert!(
        !result.faces.is_empty(),
        "metadata-only scan must find faces"
    );
    let face = &result.faces[0];
    assert!(!face.family.is_empty(), "family must be populated");
}

#[test]
fn scan_dirs_metadata_only_family_matches_full_scan() {
    let tmp = std::env::temp_dir().join(format!(
        "oxifont_meta_only_{}_{}",
        std::process::id(),
        "cmp"
    ));
    let _ = std::fs::create_dir_all(&tmp);
    std::fs::write(tmp.join("test.ttf"), TTF_FIXTURE_META).expect("write fixture");

    let partial_result = oxifont_discovery::scan_dirs_metadata_only(std::slice::from_ref(&tmp));
    let full_faces = oxifont_discovery::scan_dirs(std::slice::from_ref(&tmp));

    assert_eq!(
        partial_result.faces.len(),
        full_faces.len(),
        "face counts must match between partial and full scan"
    );
    if let (Some(p), Some(f)) = (partial_result.faces.first(), full_faces.first()) {
        assert_eq!(p.family, f.family, "family must match");
        assert_eq!(p.weight, f.weight, "weight must match");
        assert_eq!(p.style, f.style, "style must match");
        assert_eq!(p.post_script_name, f.post_script_name, "PS name must match");
    }

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn read_face_metadata_partial_single_file() {
    let tmp = std::env::temp_dir().join(format!(
        "oxifont_partial_{}_{}",
        std::process::id(),
        "single"
    ));
    let _ = std::fs::create_dir_all(&tmp);
    let font_path = tmp.join("test.ttf");
    std::fs::write(&font_path, TTF_FIXTURE_META).expect("write fixture");

    let faces =
        oxifont_discovery::read_face_metadata_partial(&font_path).expect("partial read must work");
    assert_eq!(faces.len(), 1, "TTF must yield exactly one face");
    let face = &faces[0];
    assert!(!face.family.is_empty(), "family must not be empty");
    assert_eq!(face.path, font_path, "path must point to the font file");
    assert_eq!(face.face_index, 0, "TTF always has face_index 0");

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn scan_dirs_metadata_only_elapsed_and_files_scanned() {
    let tmp = std::env::temp_dir().join(format!(
        "oxifont_elapsed_{}_{}",
        std::process::id(),
        "elapsed"
    ));
    let _ = std::fs::create_dir_all(&tmp);
    std::fs::write(tmp.join("test.ttf"), TTF_FIXTURE_META).expect("write fixture");

    let result = oxifont_discovery::scan_dirs_metadata_only(std::slice::from_ref(&tmp));
    // Duration is always non-negative; just ensure the field is accessible.
    let _ = result.elapsed.as_nanos();
    assert_eq!(result.files_scanned, 1, "one file must be counted");

    let _ = std::fs::remove_dir_all(&tmp);
}
