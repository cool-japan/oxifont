//! Tests for the opt-in JSON disk cache (feature = "cache").

#[cfg(feature = "cache")]
mod cache_tests {
    use oxifont_db::FontDatabase;

    static FIXTURE_BYTES: &[u8] = include_bytes!("fixtures/test.ttf");

    #[test]
    fn save_and_load_cache_roundtrip() {
        let cache_path =
            std::env::temp_dir().join(format!("oxifont_db_cache_test_{}.json", std::process::id()));

        // Build database from fixture bytes.
        let mut db = FontDatabase::new();
        let added = db.load_bytes(FIXTURE_BYTES.to_vec());
        assert!(added > 0, "fixture must load at least one face");
        let original_file_count = db
            .faces()
            .iter()
            .filter(|f| matches!(f.source, oxifont_db::Source::File(_)))
            .count();

        // For cache roundtrip we need File-sourced faces.  Load from a temp file.
        let tmp_font =
            std::env::temp_dir().join(format!("oxifont_fixture_{}.ttf", std::process::id()));
        std::fs::write(&tmp_font, FIXTURE_BYTES).expect("write tmp font");

        let mut db2 = FontDatabase::new();
        db2.load_file(&tmp_font)
            .expect("load_file must succeed for tmp font");
        let file_face_count = db2.faces().len();
        assert!(file_face_count > 0);

        // Save and reload from cache.
        db2.save_cache(&cache_path)
            .expect("save_cache must succeed");
        assert!(cache_path.exists(), "cache file must exist after save");

        let db3 = FontDatabase::load_cache(&cache_path).expect("load_cache must succeed");
        assert_eq!(
            db3.faces().len(),
            file_face_count,
            "loaded cache must have same face count as original"
        );

        // Cleanup.
        let _ = std::fs::remove_file(&cache_path);
        let _ = std::fs::remove_file(&tmp_font);
        let _ = original_file_count;
    }
}
