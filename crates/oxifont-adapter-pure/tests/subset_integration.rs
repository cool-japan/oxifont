//! Integration tests for `FontDatabase` subsetting operations (feature = "subset").
//!
//! These tests exercise `subset_face()` and `subset_face_for_web()` which
//! bridge [`FontDatabase::font_bytes`] with [`oxifont_subset::subset_font`].
//! They verify that:
//!
//! 1. Subsetting a real font file produces valid SFNT bytes (correct magic).
//! 2. The subset is smaller than or equal to the original.
//! 3. `subset_face_for_web()` produces an equal or smaller result than
//!    `subset_face()` (web preset strips hints and trims names).
//! 4. The `oxitext`-style usage pattern — pass `font_bytes()` to an external
//!    consumer — compiles and produces correct output.

#[cfg(feature = "subset")]
mod subset_tests {
    use oxifont_adapter_pure::FontDatabase;
    use oxifont_core::FontCatalog as _;
    use std::collections::BTreeSet;

    /// Path to the bundled test fixture TTF (parsed directly via `include_bytes!`).
    static FIXTURE_BYTES: &[u8] = include_bytes!("../../oxifont-parser/tests/fixtures/test.ttf");

    /// Write the fixture TTF to a temp directory and return a `FontDatabase`
    /// backed by it, together with the temp directory path (caller must clean up).
    /// A `suffix` parameter disambiguates concurrent test invocations that share
    /// the same PID — each test must pass a unique string (e.g. the test name).
    fn db_with_fixture(suffix: &str) -> (FontDatabase, std::path::PathBuf) {
        let tmp = std::env::temp_dir().join(format!(
            "oxifont_subset_integ_{}_{}",
            std::process::id(),
            suffix
        ));
        std::fs::create_dir_all(&tmp).expect("create temp dir");
        std::fs::write(tmp.join("test.ttf"), FIXTURE_BYTES).expect("write fixture");
        let db = FontDatabase::scan(&[&tmp]).expect("scan must not error");
        (db, tmp)
    }

    // -----------------------------------------------------------------------
    // Test 1 — subset_face() returns valid SFNT bytes
    // -----------------------------------------------------------------------

    #[test]
    fn subset_face_returns_valid_sfnt() {
        let (db, tmp) = db_with_fixture("returns_valid_sfnt");
        let info = match db.faces().first() {
            Some(f) => f.clone(),
            None => {
                let _ = std::fs::remove_dir_all(&tmp);
                return; // fixture produced no faces — skip
            }
        };

        let cps: BTreeSet<char> = "Hello, World!".chars().collect();
        let result = db.subset_face(&info, &cps);
        let _ = std::fs::remove_dir_all(&tmp);

        let subset_bytes = result.expect("subset_face must succeed on valid fixture");
        assert!(
            !subset_bytes.is_empty(),
            "subset_face must return non-empty bytes"
        );

        // Must start with a valid SFNT magic (TTF, OTF, or TTC).
        let is_valid_sfnt = subset_bytes.len() >= 4
            && (subset_bytes[..4] == [0x00, 0x01, 0x00, 0x00]
                || subset_bytes[..4] == *b"OTTO"
                || subset_bytes[..4] == *b"ttcf");
        assert!(
            is_valid_sfnt,
            "subset_face output must begin with a valid SFNT magic; got {:?}",
            &subset_bytes[..4.min(subset_bytes.len())]
        );
    }

    // -----------------------------------------------------------------------
    // Test 2 — subset is not larger than the original
    // -----------------------------------------------------------------------

    #[test]
    fn subset_face_is_not_larger_than_original() {
        let (db, tmp) = db_with_fixture("not_larger_than_original");
        let info = match db.faces().first() {
            Some(f) => f.clone(),
            None => {
                let _ = std::fs::remove_dir_all(&tmp);
                return;
            }
        };

        let original_len = FIXTURE_BYTES.len();
        let cps: BTreeSet<char> = "ABC".chars().collect();
        let result = db.subset_face(&info, &cps);
        let _ = std::fs::remove_dir_all(&tmp);

        let subset_bytes = result.expect("subset_face must succeed");
        assert!(
            subset_bytes.len() <= original_len,
            "subset output ({} bytes) must be ≤ original ({} bytes)",
            subset_bytes.len(),
            original_len
        );
    }

    // -----------------------------------------------------------------------
    // Test 3 — subset_face_for_web() succeeds
    // -----------------------------------------------------------------------

    #[test]
    fn subset_face_for_web_returns_valid_sfnt() {
        let (db, tmp) = db_with_fixture("for_web_valid_sfnt");
        let info = match db.faces().first() {
            Some(f) => f.clone(),
            None => {
                let _ = std::fs::remove_dir_all(&tmp);
                return;
            }
        };

        let cps: BTreeSet<char> = "Hello".chars().collect();
        let result = db.subset_face_for_web(&info, &cps);
        let _ = std::fs::remove_dir_all(&tmp);

        let web_bytes = result.expect("subset_face_for_web must succeed on valid fixture");
        assert!(
            !web_bytes.is_empty(),
            "subset_face_for_web must return non-empty bytes"
        );

        let is_valid_sfnt = web_bytes.len() >= 4
            && (web_bytes[..4] == [0x00, 0x01, 0x00, 0x00]
                || web_bytes[..4] == *b"OTTO"
                || web_bytes[..4] == *b"ttcf");
        assert!(
            is_valid_sfnt,
            "subset_face_for_web output must begin with a valid SFNT magic"
        );
    }

    // -----------------------------------------------------------------------
    // Test 4 — subset_face_for_web ≤ subset_face in size (web preset is smaller)
    // -----------------------------------------------------------------------

    #[test]
    fn subset_face_for_web_not_larger_than_default_subset() {
        let (db, tmp) = db_with_fixture("for_web_not_larger");
        let info = match db.faces().first() {
            Some(f) => f.clone(),
            None => {
                let _ = std::fs::remove_dir_all(&tmp);
                return;
            }
        };

        let cps: BTreeSet<char> = "ABCDEFGHIJ".chars().collect();
        let default_result = db.subset_face(&info, &cps);
        let web_result = db.subset_face_for_web(&info, &cps);
        let _ = std::fs::remove_dir_all(&tmp);

        let default_bytes = default_result.expect("subset_face must succeed");
        let web_bytes = web_result.expect("subset_face_for_web must succeed");

        assert!(
            web_bytes.len() <= default_bytes.len(),
            "web-preset subset ({} bytes) must be ≤ default subset ({} bytes)",
            web_bytes.len(),
            default_bytes.len()
        );
    }

    // -----------------------------------------------------------------------
    // Test 5 — oxitext-style pattern: font_bytes() → external consumer
    // -----------------------------------------------------------------------
    //
    // This test demonstrates the pattern described by the TODO item
    // "Provide font data access for oxifont-subset operations":
    // `FontDatabase::font_bytes(info)` returns raw SFNT bytes that can be
    // passed directly to `oxifont_subset::subset_font`.

    #[test]
    fn font_bytes_bridge_to_subset_crate() {
        let (db, tmp) = db_with_fixture("font_bytes_bridge");
        let info = match db.faces().first() {
            Some(f) => f.clone(),
            None => {
                let _ = std::fs::remove_dir_all(&tmp);
                return;
            }
        };

        // Retrieve raw bytes via the bridge method.
        let bytes = db
            .font_bytes(&info)
            .expect("font_bytes must succeed for an existing font file");

        // Pass the bytes directly to the oxifont-subset crate.
        let cps: BTreeSet<char> = "Hello, World!".chars().collect();
        let subset = oxifont_subset::subset_font(&bytes, &cps).expect("subset_font must succeed");

        assert!(
            !subset.is_empty(),
            "oxifont_subset::subset_font must produce non-empty output"
        );
        assert!(
            subset.len() <= bytes.len(),
            "subset ({} B) must be ≤ original ({} B)",
            subset.len(),
            bytes.len()
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }
}

// Non-feature-gated test: `font_bytes()` is always available and documents
// the oxitext-style usage pattern even without the `subset` feature.
#[test]
fn font_bytes_is_always_available_as_subset_bridge() {
    use oxifont_adapter_pure::FontDatabase;
    use oxifont_core::FontCatalog as _;

    static FIXTURE_BYTES: &[u8] = include_bytes!("../../oxifont-parser/tests/fixtures/test.ttf");

    let tmp = std::env::temp_dir().join(format!(
        "oxifont_fbavail_{}_{}",
        std::process::id(),
        "nofeature"
    ));
    std::fs::create_dir_all(&tmp).expect("create temp dir");
    std::fs::write(tmp.join("test.ttf"), FIXTURE_BYTES).expect("write fixture");

    let db = FontDatabase::scan(&[&tmp]).expect("scan must not error");
    if let Some(info) = db.faces().first() {
        let bytes = db
            .font_bytes(info)
            .expect("font_bytes must succeed for a file in the database");
        assert!(!bytes.is_empty(), "font_bytes must return non-empty bytes");
        // Verify the bridge contract: bytes start with a valid SFNT magic.
        let is_sfnt = bytes.len() >= 4
            && (bytes[..4] == [0x00, 0x01, 0x00, 0x00]
                || bytes[..4] == *b"OTTO"
                || bytes[..4] == *b"ttcf");
        assert!(is_sfnt, "font_bytes must start with a valid SFNT magic");
    }

    let _ = std::fs::remove_dir_all(&tmp);
}

// Test documenting the oxitext-style integration pattern:
// `FontDatabase` serves as the default font backend for `oxitext::Pipeline`.
// Without pulling in the `oxitext` crate (which would create a circular dep),
// we demonstrate the same API contract: build a `FontDatabase`, retrieve
// `font_bytes`, and simulate the pipeline construction path.
#[test]
fn pipeline_integration_pattern_via_font_bytes() {
    use oxifont_adapter_pure::FontDatabase;
    use oxifont_core::FontCatalog as _;

    static FIXTURE_BYTES: &[u8] = include_bytes!("../../oxifont-parser/tests/fixtures/test.ttf");

    let tmp = std::env::temp_dir().join(format!(
        "oxifont_pipeline_{}_{}",
        std::process::id(),
        "pattern"
    ));
    std::fs::create_dir_all(&tmp).expect("create temp dir");
    std::fs::write(tmp.join("test.ttf"), FIXTURE_BYTES).expect("write fixture");

    // This is the same code path as `oxitext::Pipeline::new(font_db)`:
    //   1. Obtain a `FontDatabase` (the pure-Rust backend).
    //   2. Pick the first face.
    //   3. Read the bytes.
    //   4. Pass bytes to the rendering pipeline (simulated here).
    let db = FontDatabase::system()
        .or_else(|_| FontDatabase::scan(&[&tmp]))
        .expect("must build a FontDatabase");

    let source = if db.is_empty() {
        FontDatabase::scan(&[&tmp]).expect("scan with fixture must succeed")
    } else {
        db
    };

    assert!(!source.is_empty(), "database must have at least one face");

    let first = source.faces().first().expect("must have a face");
    let bytes = source
        .font_bytes(first)
        .expect("font_bytes must succeed for the first face");

    // Simulate: a pipeline would call `Pipeline::from_bytes(&bytes)`.
    // We verify that the bytes are valid SFNT so the pipeline call would not fail.
    let is_sfnt = bytes.len() >= 4
        && (bytes[..4] == [0x00, 0x01, 0x00, 0x00]
            || bytes[..4] == *b"OTTO"
            || bytes[..4] == *b"ttcf");
    assert!(
        is_sfnt,
        "font bytes for pipeline must start with a valid SFNT magic; got {:?}",
        &bytes[..4.min(bytes.len())]
    );

    let _ = std::fs::remove_dir_all(&tmp);
}
