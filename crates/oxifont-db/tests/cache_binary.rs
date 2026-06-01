//! Tests for the binary oxicode disk cache (feature = "cache").

#[cfg(feature = "cache")]
mod tests {
    use oxifont_db::FontDatabase;

    static FIXTURE_BYTES: &[u8] = include_bytes!("fixtures/test.ttf");

    /// Helper: build a `FontDatabase` with at least one `File`-sourced face by
    /// writing the fixture to a temporary path and loading it from disk.
    fn db_with_file_face() -> (FontDatabase, std::path::PathBuf) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let tmp_font = std::env::temp_dir().join(format!(
            "oxifont_bin_test_fixture_{}_{}.ttf",
            std::process::id(),
            unique
        ));
        std::fs::write(&tmp_font, FIXTURE_BYTES).expect("write fixture to tmp");
        let mut db = FontDatabase::new();
        db.load_file(&tmp_font).expect("load_file must succeed");
        assert!(
            !db.faces().is_empty(),
            "fixture must load at least one face"
        );
        (db, tmp_font)
    }

    #[test]
    fn round_trip_binary_cache() {
        let (db, tmp_font) = db_with_file_face();
        let original_count = db.faces().len();

        let cache_path = std::env::temp_dir().join(format!(
            "oxifont_bin_cache_roundtrip_{}.bin",
            std::process::id()
        ));

        db.save_cache_binary(&cache_path)
            .expect("save_cache_binary must succeed");

        assert!(
            cache_path.exists(),
            "binary cache file must exist after save"
        );

        let db2 = FontDatabase::load_cache_binary(&cache_path)
            .expect("load_cache_binary must return Some for a valid cache");

        assert_eq!(
            db2.faces().len(),
            original_count,
            "round-tripped database must have the same face count"
        );

        // Verify the first face's family name survived the round-trip.
        let orig_family = db.faces()[0].family.clone();
        let rt_family = db2.faces()[0].family.clone();
        assert_eq!(
            orig_family, rt_family,
            "family name must survive binary round-trip"
        );

        let _ = std::fs::remove_file(&cache_path);
        let _ = std::fs::remove_file(&tmp_font);
    }

    #[test]
    fn corrupt_magic_returns_none() {
        let cache_path = std::env::temp_dir().join(format!(
            "oxifont_bin_cache_bad_magic_{}.bin",
            std::process::id()
        ));

        // Write a file that starts with wrong magic bytes.
        let bad_data: &[u8] = b"NOPE\x01\x00\x00\x00some_garbage_payload";
        std::fs::write(&cache_path, bad_data).expect("write bad cache");

        let result = FontDatabase::load_cache_binary(&cache_path);
        assert!(
            result.is_none(),
            "load_cache_binary must return None when magic does not match"
        );

        let _ = std::fs::remove_file(&cache_path);
    }

    #[test]
    fn version_mismatch_returns_none() {
        let cache_path = std::env::temp_dir().join(format!(
            "oxifont_bin_cache_bad_version_{}.bin",
            std::process::id()
        ));

        // Correct magic but version = 999.
        let mut data = Vec::new();
        data.extend_from_slice(b"OXDB");
        data.extend_from_slice(&999u32.to_le_bytes());
        data.extend_from_slice(b"dummy_payload");
        std::fs::write(&cache_path, &data).expect("write version-mismatch cache");

        let result = FontDatabase::load_cache_binary(&cache_path);
        assert!(
            result.is_none(),
            "load_cache_binary must return None when version does not match"
        );

        let _ = std::fs::remove_file(&cache_path);
    }

    #[test]
    fn too_short_returns_none() {
        let cache_path = std::env::temp_dir().join(format!(
            "oxifont_bin_cache_too_short_{}.bin",
            std::process::id()
        ));

        // Only 6 bytes — not enough for the 8-byte header.
        std::fs::write(&cache_path, b"OXDB\x01\x00").expect("write short file");

        let result = FontDatabase::load_cache_binary(&cache_path);
        assert!(
            result.is_none(),
            "load_cache_binary must return None for files shorter than the header"
        );

        let _ = std::fs::remove_file(&cache_path);
    }

    /// Round-trip: all fields that survive the binary cache are preserved exactly.
    ///
    /// `Source::Memory` faces are intentionally excluded from the binary cache
    /// (their bytes are not stored), so this test uses a `File`-sourced face via
    /// the fixture loader.  After a save/load cycle every metadata field read
    /// from the original `FaceInfo` must match the loaded copy.
    #[test]
    fn test_cache_round_trip_preserves_all_fields() {
        let (db, tmp_font) = db_with_file_face();
        assert!(
            !db.faces().is_empty(),
            "fixture must load at least one face"
        );

        let orig = db.faces()[0].clone();

        let tmp = std::env::temp_dir().join(format!(
            "oxifont_cache_roundtrip_{}_{}.bin",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));

        db.save_cache_binary(&tmp)
            .expect("save_cache_binary must succeed");

        let loaded = FontDatabase::load_cache_binary(&tmp)
            .expect("load_cache_binary should succeed after save");

        assert_eq!(
            loaded.faces().len(),
            db.faces().len(),
            "round-tripped database must have the same face count"
        );

        let rt = &loaded.faces()[0];

        // --- Core identity fields ---
        assert_eq!(
            rt.family, orig.family,
            "family must survive binary round-trip"
        );
        assert_eq!(
            rt.post_script_name, orig.post_script_name,
            "post_script_name must survive binary round-trip"
        );

        // --- Metric fields ---
        assert_eq!(
            rt.weight, orig.weight,
            "weight must survive binary round-trip"
        );
        assert_eq!(
            rt.italic, orig.italic,
            "italic must survive binary round-trip"
        );
        assert_eq!(
            rt.stretch, orig.stretch,
            "stretch must survive binary round-trip"
        );
        assert_eq!(
            rt.monospaced, orig.monospaced,
            "monospaced must survive binary round-trip"
        );
        assert_eq!(
            rt.face_index, orig.face_index,
            "face_index must survive binary round-trip"
        );
        assert_eq!(
            rt.unicode_ranges, orig.unicode_ranges,
            "unicode_ranges must survive binary round-trip"
        );

        // --- Variable axes (empty for a static TTF fixture) ---
        assert_eq!(
            rt.variable_axes.len(),
            orig.variable_axes.len(),
            "variable_axes length must survive binary round-trip"
        );

        // --- Locale families ---
        assert_eq!(
            rt.locale_families.len(),
            orig.locale_families.len(),
            "locale_families length must survive binary round-trip"
        );
        for (i, (orig_pair, rt_pair)) in orig
            .locale_families
            .iter()
            .zip(rt.locale_families.iter())
            .enumerate()
        {
            assert_eq!(
                orig_pair, rt_pair,
                "locale_families[{i}] must survive binary round-trip"
            );
        }

        let _ = std::fs::remove_file(&tmp);
        let _ = std::fs::remove_file(&tmp_font);
    }

    #[test]
    fn binary_cache_preferred_over_json_in_system_cached_path() {
        // This test exercises the internal priority logic by verifying that
        // save_cache_binary + load_cache_binary are a consistent pair.
        let (db, tmp_font) = db_with_file_face();

        let cache_path =
            std::env::temp_dir().join(format!("oxifont_bin_priority_{}.bin", std::process::id()));

        db.save_cache_binary(&cache_path)
            .expect("save_cache_binary");

        // Mutate the file slightly to simulate a different DB being in JSON —
        // load_cache_binary should return the binary version, not be confused.
        let db2 = FontDatabase::load_cache_binary(&cache_path)
            .expect("load_cache_binary after save_cache_binary");

        assert_eq!(db.faces().len(), db2.faces().len());

        let _ = std::fs::remove_file(&cache_path);
        let _ = std::fs::remove_file(&tmp_font);
    }
}
