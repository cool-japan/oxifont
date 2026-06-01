//! Integration tests for memory-mapped font file loading.
//!
//! Gated on `#[cfg(feature = "mmap")]`.

#[cfg(feature = "mmap")]
mod mmap_tests {
    use oxifont_discovery::{read_font_file_mmap, scan_dirs_mmap};
    use std::path::PathBuf;

    /// Raw bytes of the test fixture TTF embedded at compile time.
    static TTF_FIXTURE: &[u8] = include_bytes!("../../oxifont-parser/tests/fixtures/test.ttf");

    /// Write the fixture TTF to a unique temp file, returning its path.
    fn write_temp_ttf(suffix: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("oxifont_mmap_{suffix}.ttf"));
        std::fs::write(&p, TTF_FIXTURE).expect("write temp TTF");
        p
    }

    // -----------------------------------------------------------------------
    // read_font_file_mmap
    // -----------------------------------------------------------------------

    #[test]
    fn test_read_font_file_mmap_valid_file() {
        let path = write_temp_ttf("read_valid");
        let bytes = read_font_file_mmap(&path).expect("mmap must succeed for valid file");
        assert!(!bytes.is_empty(), "mapped bytes must not be empty");
        // Verify the bytes match what plain fs::read returns.
        let expected = std::fs::read(&path).expect("fs::read");
        assert_eq!(
            bytes, expected,
            "mmap and fs::read must return identical bytes"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_read_font_file_mmap_nonexistent_file() {
        let path = std::env::temp_dir().join("oxifont_mmap_nonexistent_xyzzy.ttf");
        let result = read_font_file_mmap(&path);
        assert!(result.is_err(), "mmap of nonexistent file must return Err");
    }

    // -----------------------------------------------------------------------
    // scan_dirs_mmap
    // -----------------------------------------------------------------------

    #[test]
    fn test_scan_dirs_mmap_finds_valid_font() {
        let base = std::env::temp_dir().join("oxifont_mmap_scan_test");
        let _ = std::fs::create_dir_all(&base);
        std::fs::write(base.join("test.ttf"), TTF_FIXTURE).expect("write fixture");

        let result = scan_dirs_mmap(std::slice::from_ref(&base));
        assert_eq!(result.files_scanned, 1, "one file scanned");
        assert_eq!(result.faces.len(), 1, "one face found");
        assert!(result.errors.is_empty(), "no errors for valid TTF");

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_scan_dirs_mmap_records_error_for_corrupt_file() {
        let base = std::env::temp_dir().join("oxifont_mmap_corrupt_test");
        let _ = std::fs::create_dir_all(&base);
        // Valid TTF + a corrupt one.
        std::fs::write(base.join("good.ttf"), TTF_FIXTURE).expect("write good");
        std::fs::write(base.join("bad.ttf"), b"not a font at all").expect("write bad");

        let result = scan_dirs_mmap(std::slice::from_ref(&base));
        assert_eq!(result.files_scanned, 2, "two files scanned");
        assert_eq!(result.faces.len(), 1, "one valid face");
        assert_eq!(result.errors.len(), 1, "one error for corrupt file");

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_scan_dirs_mmap_empty_dir_returns_empty() {
        let base = std::env::temp_dir().join("oxifont_mmap_empty_dir_test");
        let _ = std::fs::create_dir_all(&base);

        let result = scan_dirs_mmap(std::slice::from_ref(&base));
        assert_eq!(result.files_scanned, 0);
        assert!(result.faces.is_empty());
        assert!(result.errors.is_empty());

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn test_scan_dirs_mmap_elapsed_is_duration() {
        let base = std::env::temp_dir().join("oxifont_mmap_elapsed_test");
        let _ = std::fs::create_dir_all(&base);

        let result = scan_dirs_mmap(std::slice::from_ref(&base));
        // Duration is always non-negative; just confirm no panic.
        let _ = result.elapsed.as_nanos();

        let _ = std::fs::remove_dir_all(&base);
    }
}
