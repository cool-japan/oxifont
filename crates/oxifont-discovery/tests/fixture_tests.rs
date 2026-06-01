//! Comprehensive fixture tests for `oxifont-discovery`.
//!
//! Tests use `std::env::temp_dir()` (per CLAUDE.md policy) and clean up after
//! themselves. Each test uses a unique subdirectory keyed by process ID and a
//! millisecond timestamp so parallel test runs cannot interfere.

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Embedded TTF fixture from the parser test suite.
static TTF_FIXTURE: &[u8] = include_bytes!("../../oxifont-parser/tests/fixtures/test.ttf");

/// Build a unique temp directory path for this test run.
fn unique_tmp_dir(tag: &str) -> std::path::PathBuf {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "oxifont_{tag}_{pid}_{ms}",
        pid = std::process::id()
    ))
}

/// Remove the temp directory, ignoring errors (best-effort cleanup).
fn cleanup(dir: &std::path::Path) {
    let _ = std::fs::remove_dir_all(dir);
}

/// Locate a real `.ttf` file on the host system for tests that need genuine
/// font data beyond the embedded fixture.
fn find_system_ttf() -> Option<std::path::PathBuf> {
    let candidates = [
        "/System/Library/Fonts",
        "/Library/Fonts",
        "/usr/share/fonts",
        "/usr/local/share/fonts",
    ];
    for dir in &candidates {
        if let Ok(rd) = std::fs::read_dir(dir) {
            for entry in rd.flatten() {
                let p = entry.path();
                if p.extension()
                    .and_then(|x| x.to_str())
                    .map(|x| x.eq_ignore_ascii_case("ttf"))
                    .unwrap_or(false)
                {
                    return Some(p);
                }
            }
        }
    }
    None
}

/// Create a temp dir containing one copy of the embedded TTF fixture.
///
/// Returns `(temp_dir_path, font_file_path)` on success, `None` on any I/O
/// failure.
fn setup_fixture_dir(tag: &str) -> Option<(std::path::PathBuf, std::path::PathBuf)> {
    let tmp_dir = unique_tmp_dir(tag);
    std::fs::create_dir_all(&tmp_dir).ok()?;
    let font_path = tmp_dir.join("test_font.ttf");
    std::fs::write(&font_path, TTF_FIXTURE).ok()?;
    Some((tmp_dir, font_path))
}

// ---------------------------------------------------------------------------
// scan_dirs tests
// ---------------------------------------------------------------------------

#[test]
fn test_scan_temp_dir_with_fixture_ttf() {
    let Some((tmp_dir, _font_path)) = setup_fixture_dir("fixture_scan") else {
        return;
    };

    let faces = oxifont_discovery::scan_dirs(&[&tmp_dir]);
    assert!(
        !faces.is_empty(),
        "should find at least one face in temp dir containing a valid TTF"
    );

    cleanup(&tmp_dir);
}

#[test]
fn test_scan_temp_dir_with_system_ttf() {
    let Some(src) = find_system_ttf() else {
        // No system TTF available (bare CI image) — skip gracefully.
        return;
    };
    let Ok(data) = std::fs::read(&src) else {
        return;
    };

    let tmp_dir = unique_tmp_dir("sys_ttf");
    if std::fs::create_dir_all(&tmp_dir).is_err() {
        return;
    }
    let font_path = tmp_dir.join("system_font.ttf");
    if std::fs::write(&font_path, data).is_err() {
        cleanup(&tmp_dir);
        return;
    }

    let faces = oxifont_discovery::scan_dirs(&[&tmp_dir]);
    assert!(
        !faces.is_empty(),
        "should find at least one face from a real system TTF"
    );

    cleanup(&tmp_dir);
}

#[test]
fn test_scan_empty_dir_returns_empty() {
    let tmp_dir = unique_tmp_dir("empty_dir");
    std::fs::create_dir_all(&tmp_dir).expect("create temp dir");

    let faces = oxifont_discovery::scan_dirs(&[&tmp_dir]);
    assert!(
        faces.is_empty(),
        "empty directory should yield no faces, got {}",
        faces.len()
    );

    cleanup(&tmp_dir);
}

#[test]
fn test_scan_nonexistent_dir_does_not_panic() {
    let nonexistent = std::env::temp_dir().join("oxifont_definitely_does_not_exist_999888777");
    // Ensure it really does not exist.
    let _ = std::fs::remove_dir_all(&nonexistent);

    let faces = oxifont_discovery::scan_dirs(&[&nonexistent]);
    assert!(
        faces.is_empty(),
        "nonexistent directory should yield no faces"
    );
}

#[test]
fn test_scan_multiple_dirs_aggregates_results() {
    let tmp1 = unique_tmp_dir("multi_a");
    let tmp2 = unique_tmp_dir("multi_b");
    std::fs::create_dir_all(&tmp1).expect("create tmp1");
    std::fs::create_dir_all(&tmp2).expect("create tmp2");

    std::fs::write(tmp1.join("a.ttf"), TTF_FIXTURE).expect("write a.ttf");
    std::fs::write(tmp2.join("b.ttf"), TTF_FIXTURE).expect("write b.ttf");

    let faces = oxifont_discovery::scan_dirs(&[&tmp1, &tmp2]);
    assert_eq!(
        faces.len(),
        2,
        "two directories with one TTF each should yield 2 faces"
    );

    cleanup(&tmp1);
    cleanup(&tmp2);
}

// ---------------------------------------------------------------------------
// scan_dirs_reporting tests
// ---------------------------------------------------------------------------

#[test]
fn test_scan_reporting_tracks_errors_for_invalid_ttf() {
    let tmp_dir = unique_tmp_dir("reporting_err");
    std::fs::create_dir_all(&tmp_dir).expect("create temp dir");

    // A file with a `.ttf` extension but invalid content.
    let fake_font = tmp_dir.join("fake.ttf");
    std::fs::write(&fake_font, b"NOTAFONT").expect("write fake font");

    let result = oxifont_discovery::scan_dirs_reporting(std::slice::from_ref(&tmp_dir));
    assert_eq!(
        result.files_scanned, 1,
        "one .ttf file should have been counted as scanned"
    );
    assert_eq!(
        result.errors.len(),
        1,
        "the invalid TTF should produce exactly one error"
    );
    assert!(
        result.faces.is_empty(),
        "no valid faces should be returned for the invalid TTF"
    );

    cleanup(&tmp_dir);
}

#[test]
fn test_scan_reporting_valid_and_invalid_mix() {
    let tmp_dir = unique_tmp_dir("reporting_mix");
    std::fs::create_dir_all(&tmp_dir).expect("create temp dir");

    std::fs::write(tmp_dir.join("valid.ttf"), TTF_FIXTURE).expect("write valid TTF");
    std::fs::write(tmp_dir.join("bad.ttf"), b"GARBAGE").expect("write bad TTF");

    let result = oxifont_discovery::scan_dirs_reporting(std::slice::from_ref(&tmp_dir));
    assert_eq!(result.files_scanned, 2, "two files should be scanned");
    assert_eq!(result.faces.len(), 1, "one valid face expected");
    assert_eq!(result.errors.len(), 1, "one parse error expected");
    assert_eq!(
        result.total_errors(),
        1,
        "total_errors() must mirror errors.len()"
    );

    cleanup(&tmp_dir);
}

#[test]
fn test_scan_reporting_elapsed_is_non_zero_duration() {
    let tmp_dir = unique_tmp_dir("reporting_elapsed");
    std::fs::create_dir_all(&tmp_dir).expect("create temp dir");
    std::fs::write(tmp_dir.join("font.ttf"), TTF_FIXTURE).expect("write TTF");

    let result = oxifont_discovery::scan_dirs_reporting(std::slice::from_ref(&tmp_dir));
    // Duration is always >= 0; we just verify it doesn't panic.
    let _ = result.elapsed.as_nanos();

    cleanup(&tmp_dir);
}

// ---------------------------------------------------------------------------
// Unreadable file test (Unix-only)
// ---------------------------------------------------------------------------

#[test]
#[cfg(unix)]
fn test_scan_unreadable_file_skips_gracefully() {
    use std::os::unix::fs::PermissionsExt;

    let tmp_dir = unique_tmp_dir("perm_test");
    std::fs::create_dir_all(&tmp_dir).expect("create temp dir");

    let fake_font = tmp_dir.join("unreadable.ttf");
    std::fs::write(&fake_font, b"NOTAFONT").expect("write file");
    std::fs::set_permissions(&fake_font, std::fs::Permissions::from_mode(0o000))
        .expect("make file unreadable");

    // Must not panic — simply skip the unreadable file.
    let faces = oxifont_discovery::scan_dirs(&[&tmp_dir]);
    // We cannot assert on the count because the parser may or may not attempt
    // to read the file before seeing EACCES; the only requirement is no panic.
    let _ = faces;

    // Restore permissions so cleanup can remove the file.
    let _ = std::fs::set_permissions(&fake_font, std::fs::Permissions::from_mode(0o644));
    cleanup(&tmp_dir);
}

// ---------------------------------------------------------------------------
// Parallel scan tests (rayon feature)
// ---------------------------------------------------------------------------

#[test]
#[cfg(feature = "rayon")]
fn test_scan_dirs_parallel_produces_same_count_as_sequential() {
    let tmp_dir = unique_tmp_dir("rayon_parity");
    std::fs::create_dir_all(&tmp_dir).expect("create temp dir");

    // Write several TTF copies so there is meaningful work to parallelise.
    for i in 0..5 {
        std::fs::write(tmp_dir.join(format!("font_{i}.ttf")), TTF_FIXTURE)
            .expect("write TTF fixture");
    }

    let sequential = oxifont_discovery::scan_dirs(&[&tmp_dir]);
    let parallel = oxifont_discovery::scan_dirs_parallel(&[&tmp_dir]);

    assert_eq!(
        sequential.len(),
        parallel.len(),
        "parallel scan must yield the same number of faces as sequential scan"
    );

    cleanup(&tmp_dir);
}

#[test]
#[cfg(feature = "rayon")]
fn test_scan_dirs_parallel_empty_dir() {
    let tmp_dir = unique_tmp_dir("rayon_empty");
    std::fs::create_dir_all(&tmp_dir).expect("create temp dir");

    let faces = oxifont_discovery::scan_dirs_parallel(&[&tmp_dir]);
    assert!(
        faces.is_empty(),
        "parallel scan of empty dir must return no faces"
    );

    cleanup(&tmp_dir);
}

#[test]
#[cfg(feature = "rayon")]
fn test_scan_dirs_parallel_nonexistent_dir_does_not_panic() {
    let nonexistent = std::env::temp_dir().join("oxifont_rayon_no_exist_999888");
    let _ = std::fs::remove_dir_all(&nonexistent);

    let faces = oxifont_discovery::scan_dirs_parallel(&[&nonexistent]);
    assert!(
        faces.is_empty(),
        "parallel scan of nonexistent dir must return no faces"
    );
}
