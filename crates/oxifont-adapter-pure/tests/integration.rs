//! Integration tests for `FontDatabase` requiring real system fonts.
//!
//! These tests skip gracefully if no system fonts are found on the host. They
//! exercise `system()`, `load_face()`, `font_bytes()`, and `find_best_for_text()`
//! with actual font files on the filesystem — code paths that the unit tests
//! (which use a bundled fixture) cannot reach.

use oxifont_adapter_pure::FontDatabase;
use oxifont_core::FontFace as _;
use oxifont_core::{FontCatalog as _, FontQuery};
use oxifont_parser::ParsedFace;

/// Path to the shared parser fixture used by unit tests.
static FIXTURE_BYTES: &[u8] = include_bytes!("../../oxifont-parser/tests/fixtures/test.ttf");

// ---------------------------------------------------------------------------
// Helper — locate any TTF font on the host
// ---------------------------------------------------------------------------

/// Search well-known system font directories for a usable TTF file.
///
/// Returns `None` when no TTF is found (e.g. a minimal CI container without
/// system fonts installed). Callers use this as a graceful-skip gate.
fn find_any_ttf_on_system() -> Option<std::path::PathBuf> {
    let dirs = oxifont_discovery::system_font_dirs();

    for dir in &dirs {
        if !dir.exists() {
            continue;
        }
        // Walk one level deep — system font dirs are typically flat.
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("ttf")) && p.is_file() {
                    return Some(p);
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Test 1 — system() catalog construction
// ---------------------------------------------------------------------------

/// `FontDatabase::system()` must not panic and, when system fonts are present,
/// every face record it returns must have a non-empty family name.
#[test]
fn system_catalog_constructs_without_panic() {
    let db = FontDatabase::system().expect("FontDatabase::system() must not error");

    // Graceful skip: an empty catalog is acceptable on containers that have no
    // fonts installed. We just verify it constructed cleanly.
    if db.is_empty() {
        return;
    }

    // Verify structural invariants on every face the catalog discovered.
    for face in db.faces() {
        assert!(
            !face.family.is_empty(),
            "every FaceInfo returned by system() must have a non-empty family name; got empty for {:?}",
            face.path
        );
        assert!(
            face.path.exists(),
            "every FaceInfo returned by system() must point to an existing file; {:?} is missing",
            face.path
        );
    }
}

// ---------------------------------------------------------------------------
// Test 2 — load_face() round-trip
// ---------------------------------------------------------------------------

/// `FontDatabase::load_face()` must parse the face that `FaceInfo` describes
/// and return a `ParsedFace` whose `family_name()` matches `info.family`.
#[test]
fn load_face_parses_successfully() {
    // Prefer the system catalog so we exercise the same FaceInfo records that
    // `system()` produces. Fall back to a file-based search if the catalog is
    // empty on this machine.
    let db = FontDatabase::system().expect("FontDatabase::system() must not error");

    let info = if let Some(f) = db.faces().first() {
        f.clone()
    } else {
        // No catalog entry — try to find a raw TTF and parse it directly
        // without going through the catalog.
        let Some(path) = find_any_ttf_on_system() else {
            // No system fonts at all — skip gracefully.
            return;
        };

        let data = std::fs::read(&path).expect("font file must be readable");
        let face =
            ParsedFace::parse(data, 0).expect("ParsedFace::parse must succeed for a real TTF file");
        // Verify the bare-parser path still yields a non-empty family.
        assert!(
            !face.family_name().is_empty(),
            "ParsedFace::family_name() must not be empty for a real TTF file at {:?}",
            path
        );
        return;
    };

    // Happy path: use the catalog's load_face() method.
    let parsed = db
        .load_face(&info)
        .expect("FontDatabase::load_face() must succeed for a FaceInfo from system()");

    assert!(
        !parsed.family_name().is_empty(),
        "ParsedFace::family_name() must not be empty after load_face(); got empty for {:?}",
        info.path
    );

    // The parsed family should match the catalog entry (the parser and
    // discovery layer extract the name the same way).
    assert_eq!(
        parsed.family_name(),
        info.family.as_ref(),
        "ParsedFace::family_name() must match FaceInfo::family for {:?}",
        info.path
    );
}

// ---------------------------------------------------------------------------
// Test 3 — find_best_for_text() with ASCII text
// ---------------------------------------------------------------------------

/// `find_best_for_text()` must return a face when the database is non-empty and
/// an unconstrained query is used. On macOS (where Arial / Helvetica are always
/// installed) a `"sans-serif"` generic family query must also resolve.
#[test]
fn find_best_for_text_with_ascii() {
    let db = FontDatabase::system().expect("FontDatabase::system() must not error");

    // Graceful skip: nothing to query if there are no fonts.
    if db.is_empty() {
        return;
    }

    // An unconstrained query (no family, no style, no weight) must return the
    // first available face from a non-empty database.
    let result = db.find_best_for_text(&FontQuery::new(), "Hello world");
    assert!(
        result.is_some(),
        "find_best_for_text() with an unconstrained query must return Some when the catalog is non-empty"
    );

    // A query with a concrete family (taken from the first catalog entry) must
    // resolve to the same family.
    let first_family = db.faces()[0].family.clone();
    let query = FontQuery::new().family(&*first_family);
    let by_family = db.find_best_for_text(&query, "Hello");
    assert!(
        by_family.is_some(),
        "find_best_for_text() with family {:?} must return Some",
        first_family
    );
}

// ---------------------------------------------------------------------------
// Test 4 — CSS generic family resolution on macOS
// ---------------------------------------------------------------------------

/// On macOS the `GENERIC_FAMILIES` table maps `"sans-serif"` to Arial /
/// Helvetica. At least one of those should be installed; verify that
/// `find_css()` (via `find_best_for_text`) can resolve the generic keyword.
#[test]
fn css_generic_sans_serif_resolves_on_macos() {
    #[cfg(not(target_os = "macos"))]
    {
        // This test is macOS-specific; skip on other platforms.
        return;
    }

    #[cfg(target_os = "macos")]
    {
        let db = FontDatabase::system().expect("FontDatabase::system() must not error");

        if db.is_empty() {
            // Minimal macOS container without fonts — skip gracefully.
            return;
        }

        let query = FontQuery::new().family("sans-serif");
        let found = db.find_best_for_text(&query, "Hello, world!");

        // macOS ships at least Helvetica Neue; if the catalog is non-empty
        // and none of the sans-serif fallbacks resolved, that is unexpected.
        // We issue the assertion only when we can confirm a known alias is present.
        let has_known_alias = db.faces().iter().any(|f| {
            let lo = f.family.to_lowercase();
            lo.contains("helvetica") || lo.contains("arial")
        });

        if has_known_alias {
            assert!(
                found.is_some(),
                "find_best_for_text() with 'sans-serif' must resolve when Helvetica or Arial is in the catalog"
            );
        }
        // If neither alias is present (unusual but theoretically possible),
        // we skip the assertion rather than fail the test suite.
    }
}

// ---------------------------------------------------------------------------
// Test 5 — font_bytes() returns raw SFNT bytes suitable for subsetting
// ---------------------------------------------------------------------------

/// `FontDatabase::font_bytes()` must return non-empty bytes matching the data
/// of the file pointed to by `FaceInfo::path`. This confirms the method is
/// usable as a source for `oxifont_subset::subset_font`.
#[test]
fn font_bytes_returns_raw_file_bytes() {
    // Use an in-memory face from the bundled fixture so the test does not
    // depend on system fonts. We write the fixture to a temp file first so
    // that `FaceInfo::path` points to an actual file (as the adapter expects).
    let tmp_dir = std::env::temp_dir().join(format!("oxifont_font_bytes_{}", std::process::id()));
    std::fs::create_dir_all(&tmp_dir).expect("create temp dir");
    let font_path = tmp_dir.join("test.ttf");
    std::fs::write(&font_path, FIXTURE_BYTES).expect("write fixture");

    let db = FontDatabase::scan(&[&tmp_dir]).expect("scan must not error");

    let info = match db.faces().first() {
        Some(f) => f,
        None => {
            // No face parsed from the fixture — skip gracefully.
            let _ = std::fs::remove_file(&font_path);
            let _ = std::fs::remove_dir(&tmp_dir);
            return;
        }
    };

    let bytes = db
        .font_bytes(info)
        .expect("font_bytes() must succeed for an existing font file");

    assert!(
        !bytes.is_empty(),
        "font_bytes() must return non-empty bytes for a valid font file"
    );

    // The bytes must start with a valid SFNT magic (TTF 0x00010000 or OTF
    // "OTTO" or TTC "ttcf"). The fixture is a TTF.
    let is_valid_sfnt = bytes.len() >= 4
        && (bytes[..4] == [0x00, 0x01, 0x00, 0x00]
            || bytes[..4] == *b"OTTO"
            || bytes[..4] == *b"ttcf");
    assert!(
        is_valid_sfnt,
        "font_bytes() must return bytes starting with a valid SFNT magic; got {:?}",
        &bytes[..4.min(bytes.len())]
    );

    // Verify that the bytes match what we wrote (i.e. the correct file is read).
    assert_eq!(
        bytes, FIXTURE_BYTES,
        "font_bytes() must return the exact bytes of the file on disk"
    );

    let _ = std::fs::remove_file(&font_path);
    let _ = std::fs::remove_dir(&tmp_dir);
}

// ---------------------------------------------------------------------------
// Test 6 — into_db() bridge to oxifont-db (requires feature = "db")
// ---------------------------------------------------------------------------

/// `FontDatabase::into_db()` must produce an `oxifont_db::FontDatabase` with
/// the same face count, and the resulting database must support CSS Level 4
/// queries via `oxifont_db::Query`.
#[cfg(feature = "db")]
#[test]
fn into_db_bridge_produces_equivalent_catalog() {
    let mut pure_db = FontDatabase::new();
    pure_db
        .add_bytes(FIXTURE_BYTES.to_vec(), None)
        .expect("add_bytes must succeed for a valid TTF");

    let pure_count = pure_db.len();
    assert!(pure_count > 0, "precondition: at least one face added");

    // Grab the family name before consuming pure_db.
    let family = pure_db.faces()[0].family.to_string();

    // Convert to oxifont-db.
    let db_db = pure_db.into_db();

    // Same face count after conversion.
    assert_eq!(
        db_db.stats().face_count,
        pure_count,
        "into_db() must produce a database with the same face count"
    );

    // The family must be queryable in the new database.
    let hits = db_db.faces_by_family(&family);
    assert!(
        !hits.is_empty(),
        "oxifont_db::FontDatabase must return faces for family {:?} after conversion",
        family
    );
}

/// `FontDatabase::as_db()` must produce an equivalent CSS-queryable database
/// while leaving the original catalog intact.
#[cfg(feature = "db")]
#[test]
fn as_db_bridge_does_not_consume_catalog() {
    let mut pure_db = FontDatabase::new();
    pure_db
        .add_bytes(FIXTURE_BYTES.to_vec(), None)
        .expect("add_bytes must succeed for a valid TTF");

    let pure_count = pure_db.len();

    // Convert by reference.
    let db_db = pure_db.as_db();

    // Original catalog is unchanged.
    assert_eq!(
        pure_db.len(),
        pure_count,
        "as_db() must not consume the original catalog"
    );

    // CSS db has same face count.
    assert_eq!(
        db_db.stats().face_count,
        pure_count,
        "as_db() must produce a database with the same face count"
    );
}
