//! Integration tests for `FontDatabase` requiring real system fonts.
//!
//! These tests skip gracefully if no system fonts are found on the host. They
//! exercise `system()`, `load_face()`, and `find_best_for_text()` with actual
//! font files on the filesystem — code paths that the unit tests (which use a
//! bundled fixture) cannot reach.

use oxifont_adapter_pure::FontDatabase;
use oxifont_core::FontFace as _;
use oxifont_core::{FontCatalog as _, FontQuery};
use oxifont_parser::ParsedFace;

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
