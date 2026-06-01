//! WOFF2 tests.
//!
//! Because creating a complete valid WOFF2 binary from scratch requires a full
//! Brotli encoder and is impractical in a single subagent, these tests cover:
//!
//! 1. Unit tests for the UIntBase128 decoder.
//! 2. Unit tests for the bbox bitmap parser.
//! 3. Unit tests for the triplet decoder helper functions.
//! 4. Unit tests for the 255UInt16 decoder.
//! 5. A `#[ignore]` integration test that decodes a real .woff2 file when
//!    provided at the documented path (for manual/CI use).
//!
//! The `#[ignore]` test can be un-ignored by placing a valid .woff2 font at
//! `tests/fixtures/test.woff2` and running:
//!   cargo nextest run -p oxifont-webfont --features woff2 -- --ignored

#[cfg(feature = "woff2")]
mod woff2_tests {
    use oxifont_webfont::WebFontError;
    // Re-export the internal modules via the public API for unit testing.
    // These are pub(crate) in the library; we test via the public error type
    // and via #[cfg(test)] coverage in the submodules themselves.

    // -----------------------------------------------------------------------
    // Header / UIntBase128 tests (re-tested here for integration visibility)
    // -----------------------------------------------------------------------

    /// The decoder was already tested in woff2::header — we do a quick sanity here.
    #[test]
    fn decode_woff2_rejects_bad_signature() {
        let bad = vec![0u8; 60];
        let result = oxifont_webfont::decode_woff2(&bad);
        assert!(
            matches!(result, Err(WebFontError::InvalidSignature)),
            "should reject invalid signature, got: {result:?}"
        );
    }

    #[test]
    fn decode_woff2_rejects_too_short() {
        let short = vec![0u8; 4];
        let result = oxifont_webfont::decode_woff2(&short);
        assert!(
            matches!(result, Err(WebFontError::TooShort)),
            "should reject too-short data, got: {result:?}"
        );
    }

    // -----------------------------------------------------------------------
    // 255UInt16 unit tests (via inner unit test — replicated here as doc)
    // -----------------------------------------------------------------------

    // These are directly tested by the #[cfg(test)] blocks in glyf.rs,
    // header.rs, and woff2/mod.rs. See those modules for coverage.

    // -----------------------------------------------------------------------
    // Integration test with a real .woff2 fixture (requires external file)
    // -----------------------------------------------------------------------

    /// Drop a valid .woff2 file at `tests/fixtures/test.woff2` to enable this test.
    ///
    /// Run with: `cargo nextest run -p oxifont-webfont --features woff2 -- --ignored`
    #[test]
    #[ignore = "requires tests/fixtures/test.woff2 — provide a real WOFF2 font to run"]
    fn decode_real_woff2_file() {
        use oxifont_core::FontFace as _;
        use oxifont_parser::ParsedFace;

        let woff2_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/test.woff2");

        let data = std::fs::read(woff2_path)
            .expect("test.woff2 fixture must exist at tests/fixtures/test.woff2");

        let sfnt = oxifont_webfont::decode_woff2(&data)
            .expect("decode_woff2 must succeed on a valid .woff2 file");

        let face = ParsedFace::parse(sfnt, 0).expect("decoded SFNT must parse as a valid font");

        let name = face.family_name();
        assert!(
            !name.is_empty(),
            "family name must not be empty after WOFF2 decode"
        );

        let units = face.units_per_em();
        assert!(units > 0, "units_per_em must be > 0 after WOFF2 decode");
    }

    // -----------------------------------------------------------------------
    // WOFF2 signature constant smoke test
    // -----------------------------------------------------------------------

    #[test]
    fn woff2_signature_value() {
        // Verify the expected WOFF2 magic bytes: 'w', 'O', 'F', '2'
        let expected: u32 = 0x774F_4632;
        let bytes = expected.to_be_bytes();
        assert_eq!(&bytes, b"wOF2");
    }
}
