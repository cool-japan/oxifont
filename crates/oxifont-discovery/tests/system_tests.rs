//! System-level integration tests for `oxifont-discovery`.
//!
//! Covers three scenarios from the TODO list:
//!   1. `system_font_dirs()` returns a non-empty list on macOS (already
//!      tested in `discovery.rs`; this file holds the canonical TODO-closure
//!      test for the macOS CI assertion).
//!   2. TTC collection scanning yields multiple `FaceInfo` entries per file.
//!   3. Symlink following works correctly.

// ---------------------------------------------------------------------------
// Embedded fixture (same as used in fixture_tests.rs)
// ---------------------------------------------------------------------------

static TTF_FIXTURE: &[u8] = include_bytes!("../../oxifont-parser/tests/fixtures/test.ttf");

/// Build a unique temp directory path for this test run.
fn unique_tmp_dir(tag: &str) -> std::path::PathBuf {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "oxifont_sys_{tag}_{pid}_{ms}",
        pid = std::process::id()
    ))
}

/// Remove the temp directory, ignoring errors (best-effort cleanup).
fn cleanup(dir: &std::path::Path) {
    let _ = std::fs::remove_dir_all(dir);
}

// ---------------------------------------------------------------------------
// 1. system_font_dirs() non-empty
// ---------------------------------------------------------------------------

/// `system_font_dirs()` must return at least one directory on macOS.
///
/// On other platforms the function is allowed to return an empty vec (e.g.
/// inside a minimal CI container that has no fonts installed), so we only
/// assert non-empty on macOS where `/System/Library/Fonts` always exists.
#[test]
fn system_font_dirs_non_empty() {
    let dirs = oxifont_discovery::system_font_dirs();

    #[cfg(target_os = "macos")]
    assert!(
        !dirs.is_empty(),
        "macOS must have at least one system font directory"
    );

    // On all platforms: must not panic, must return a Vec.
    let _ = dirs;
}

// ---------------------------------------------------------------------------
// 2. TTC collection scanning
// ---------------------------------------------------------------------------

/// Search common system locations for a TTC / OTC collection file.
///
/// Returns `None` when no collection file can be found (e.g. bare CI
/// container without system fonts), allowing the test to skip gracefully.
fn find_collection_font() -> Option<std::path::PathBuf> {
    let search_dirs: &[&str] = &[
        "/System/Library/Fonts",
        "/System/Library/Fonts/Supplemental",
        "/Library/Fonts",
        "/usr/share/fonts",
        "/usr/local/share/fonts",
    ];

    for dir in search_dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("ttc") || e.eq_ignore_ascii_case("otc"))
            {
                return Some(p);
            }
        }
    }
    None
}

/// Scanning a TTC/OTC collection file must yield at least 2 `FaceInfo`
/// entries, one per sub-face, each with a distinct `face_index`.
#[test]
fn ttc_file_produces_multiple_face_entries() {
    let Some(collection_path) = find_collection_font() else {
        // No collection file available on this host — skip gracefully.
        return;
    };

    let dir = collection_path
        .parent()
        .expect("collection file must have a parent directory");

    let result = oxifont_discovery::scan_dirs_reporting(std::slice::from_ref(&dir.to_path_buf()));

    // Filter to faces that came from the specific collection file we found.
    let collection_faces: Vec<_> = result
        .faces
        .iter()
        .filter(|f| f.path == collection_path)
        .collect();

    assert!(
        collection_faces.len() >= 2,
        "TTC/OTC at {} should produce at least 2 faces, got {}",
        collection_path.display(),
        collection_faces.len()
    );

    // Each sub-face must carry a distinct face_index.
    let indices: std::collections::HashSet<u32> =
        collection_faces.iter().map(|f| f.face_index).collect();
    assert_eq!(
        indices.len(),
        collection_faces.len(),
        "every sub-face in a TTC/OTC must have a unique face_index"
    );
}

// ---------------------------------------------------------------------------
// 3. Symlink following
// ---------------------------------------------------------------------------

/// Symlinks to font files must be followed and the font must be discovered.
///
/// Strategy: write the embedded TTF fixture into a temp file, create a
/// symlink to it in a separate temp directory, then scan the symlink
/// directory. The scan must return at least one face. Uses the fixture
/// (no system font search needed) — deterministic on any Unix host.
#[cfg(unix)]
#[test]
fn symlink_font_files_are_followed() {
    use std::os::unix::fs::symlink;

    // Write the fixture TTF to a named file so we have a real path to link.
    let src_dir = unique_tmp_dir("symlink_src");
    std::fs::create_dir_all(&src_dir).expect("create src temp dir");
    let real_font = src_dir.join("real_font.ttf");
    std::fs::write(&real_font, TTF_FIXTURE).expect("write fixture TTF");

    // Create a separate temp dir containing only a symlink to the real font.
    let link_dir = unique_tmp_dir("symlink_link");
    std::fs::create_dir_all(&link_dir).expect("create link temp dir");
    let link_path = link_dir.join("linked_font.ttf");
    symlink(&real_font, &link_path).expect("create symlink to font");

    // Scan the directory containing the symlink.
    // Default ScanOptions has follow_symlinks = true.
    let faces = oxifont_discovery::scan_dirs(&[&link_dir]);

    // Clean up before asserting so the cleanup always runs.
    cleanup(&src_dir);
    cleanup(&link_dir);

    assert!(
        !faces.is_empty(),
        "should have found at least one face through the symlink, got {}",
        faces.len()
    );
}
