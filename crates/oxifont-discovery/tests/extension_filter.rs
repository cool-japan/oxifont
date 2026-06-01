//! Integration tests verifying that non-font files are skipped by extension
//! pre-filtering before any file I/O is attempted.

/// Verify that files without a known font extension are skipped entirely.
///
/// The directory contains only README.txt and LICENSE (no extension), neither
/// of which should trigger a parse attempt.  The resulting face list must be
/// empty because no font-extension files exist.
#[test]
fn non_font_files_are_skipped() {
    let dir = std::env::temp_dir().join(format!("oxifont_ext_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create dir");

    std::fs::write(dir.join("README.txt"), b"Not a font").expect("write README.txt");
    std::fs::write(dir.join("LICENSE"), b"MIT license").expect("write LICENSE");

    let result = oxifont_discovery::scan_dirs(std::slice::from_ref(&dir));
    assert_eq!(
        result.len(),
        0,
        "Non-font files must be skipped — expected 0 faces, got {}",
        result.len()
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// Verify that a file with a `.ttf` extension is attempted for parsing even
/// when its contents are garbage.  The parse will fail gracefully and the face
/// list will be empty — but the important invariant is that no panic or I/O
/// error propagates to the caller.
#[test]
fn font_extension_file_attempted_and_fails_gracefully() {
    let dir = std::env::temp_dir().join(format!("oxifont_ext2_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create dir");

    std::fs::write(dir.join("fake.ttf"), b"not a real font").expect("write fake.ttf");

    // scan_dirs silently drops parse errors; we just need to confirm no panic.
    let result = oxifont_discovery::scan_dirs(std::slice::from_ref(&dir));
    assert_eq!(
        result.len(),
        0,
        "Garbage TTF must parse to 0 faces, got {}",
        result.len()
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// Verify that font-extension files are attempted for parsing while non-font
/// files are skipped, by scanning a mixed directory and observing that only
/// font-extension files contribute to `files_scanned`.
#[test]
fn scan_options_extension_classification() {
    let dir = std::env::temp_dir().join(format!("oxifont_ext_class_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create dir");

    // These should be skipped (non-font extensions).
    std::fs::write(dir.join("notes.txt"), b"text").expect("write notes.txt");
    std::fs::write(dir.join("data.json"), b"{}").expect("write data.json");
    std::fs::write(dir.join("image.png"), b"\x89PNG").expect("write image.png");

    // These should be attempted (font extensions, even though content is garbage).
    std::fs::write(dir.join("a.ttf"), b"garbage").expect("write a.ttf");
    std::fs::write(dir.join("b.otf"), b"garbage").expect("write b.otf");
    std::fs::write(dir.join("c.woff"), b"garbage").expect("write c.woff");

    let result = oxifont_discovery::scan_dirs_reporting(std::slice::from_ref(&dir));

    // Only the 3 font-extension files should be counted as scanned.
    assert_eq!(
        result.files_scanned, 3,
        "files_scanned must count only font-extension files, got {}",
        result.files_scanned
    );
    // All 3 will fail to parse — no valid faces.
    assert_eq!(result.faces.len(), 0, "garbage files must yield 0 faces");

    let _ = std::fs::remove_dir_all(&dir);
}

/// Verify that `scan_dirs_reporting` also skips non-font files: `files_scanned`
/// must be zero when the directory contains only non-font-extension files.
#[test]
fn reporting_skips_non_font_files() {
    let dir = std::env::temp_dir().join(format!("oxifont_ext3_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create dir");

    std::fs::write(dir.join("notes.txt"), b"some notes").expect("write notes.txt");
    std::fs::write(dir.join("data.json"), b"{}").expect("write data.json");

    let result = oxifont_discovery::scan_dirs_reporting(std::slice::from_ref(&dir));
    assert_eq!(
        result.files_scanned, 0,
        "files_scanned must be 0 when no font-extension files are present"
    );
    assert_eq!(result.faces.len(), 0, "faces must be empty");
    assert_eq!(result.errors.len(), 0, "errors must be empty");

    let _ = std::fs::remove_dir_all(&dir);
}
