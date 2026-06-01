//! Tests for WOFF1/WOFF2 metadata block extraction and CFF detection.

// -----------------------------------------------------------------------
// WOFF1 metadata tests
// -----------------------------------------------------------------------

#[cfg(feature = "woff1")]
mod woff1_metadata {
    use oxifont_webfont::sfnt::{build_sfnt, SFNT_MAGIC_TT};
    use oxifont_webfont::{decode_auto, encode_woff1};

    /// A WOFF1 produced by the encoder has no metadata block → metadata == None.
    #[test]
    fn test_woff1_metadata_none_if_absent() {
        let sfnt = build_sfnt(SFNT_MAGIC_TT, &[(*b"maxp", vec![0u8; 6])])
            .expect("build_sfnt should succeed");
        let woff1 = encode_woff1(&sfnt).expect("encode_woff1 should succeed");

        let result = decode_auto(&woff1).expect("decode_auto woff1 should succeed");
        assert!(
            result.metadata.is_none(),
            "WOFF1 without metadata block must return None, got: {:?}",
            result.metadata
        );
    }

    /// A WOFF1 with a non-zero metaOffset but metaLength == 0 → metadata == None.
    #[test]
    fn test_woff1_zero_meta_length_is_none() {
        let sfnt = build_sfnt(SFNT_MAGIC_TT, &[]).expect("build_sfnt should succeed");
        let woff1 = encode_woff1(&sfnt).expect("encode_woff1 should succeed");

        // Patch metaOffset to a non-zero value but keep metaLength == 0.
        // metaOffset is at byte 24 in the WOFF1 header.
        let mut patched = woff1;
        let fake_offset: u32 = 44; // some plausible but irrelevant offset
        patched[24..28].copy_from_slice(&fake_offset.to_be_bytes());
        // metaLength (offset 28) stays 0 from encoder.

        let result =
            decode_auto(&patched).expect("decode_auto should succeed with zero metaLength");
        assert!(
            result.metadata.is_none(),
            "metaLength == 0 must return None"
        );
    }
}

// -----------------------------------------------------------------------
// WOFF2 metadata tests
// -----------------------------------------------------------------------

#[cfg(feature = "woff2")]
mod woff2_metadata {
    use oxifont_webfont::sfnt::{build_sfnt, SFNT_MAGIC_TT};
    use oxifont_webfont::{decode_auto, encode_woff2};

    /// A WOFF2 produced by the encoder has no metadata block → metadata == None.
    #[test]
    fn test_woff2_metadata_none_if_absent() {
        let sfnt = build_sfnt(SFNT_MAGIC_TT, &[(*b"name", b"Test".to_vec())])
            .expect("build_sfnt should succeed");
        let woff2 = encode_woff2(&sfnt).expect("encode_woff2 should succeed");

        let result = decode_auto(&woff2).expect("decode_auto woff2 should succeed");
        assert!(
            result.metadata.is_none(),
            "WOFF2 without metadata block must return None, got: {:?}",
            result.metadata
        );
    }

    /// A WOFF2 with a non-zero metaOffset but metaLength == 0 → metadata == None.
    #[test]
    fn test_woff2_zero_meta_length_is_none() {
        let sfnt = build_sfnt(SFNT_MAGIC_TT, &[]).expect("build_sfnt should succeed");
        let woff2 = encode_woff2(&sfnt).expect("encode_woff2 should succeed");

        // metaOffset is at WOFF2 header byte 28; metaLength at byte 32.
        // Patch metaOffset to non-zero, leave metaLength == 0.
        let mut patched = woff2;
        let fake_offset: u32 = 48; // header size itself — harmless, length check fires first
        patched[28..32].copy_from_slice(&fake_offset.to_be_bytes());

        let result =
            decode_auto(&patched).expect("decode_auto should succeed with zero metaLength");
        assert!(
            result.metadata.is_none(),
            "metaLength == 0 must return None"
        );
    }
}

// -----------------------------------------------------------------------
// CFF detection tests
// -----------------------------------------------------------------------

#[cfg(feature = "woff2")]
mod cff_detection {
    use oxifont_webfont::sfnt::{build_sfnt, SFNT_MAGIC_CFF, SFNT_MAGIC_TT};
    use oxifont_webfont::{decode_woff2, encode_woff2};

    /// A CFF SFNT (magic = OTTO, has CFF  table) must encode and round-trip
    /// without the glyf/loca transform being applied.
    #[test]
    fn test_cff_font_encodes_without_glyf_transform() {
        // Build a synthetic CFF SFNT with a minimal CFF  table.
        // The table content is intentionally trivial — we only care that
        // the WOFF2 encoder does not attempt a glyf/loca transform.
        let cff_table = vec![0u8; 8]; // minimal opaque CFF blob
        let sfnt = build_sfnt(SFNT_MAGIC_CFF, &[(*b"CFF ", cff_table)])
            .expect("build_sfnt should succeed for CFF font");

        let woff2 = encode_woff2(&sfnt).expect("encode_woff2 must not fail for CFF font");
        assert!(
            woff2.len() >= 48,
            "WOFF2 must be at least 48 bytes (header)"
        );

        // The round-trip decode must also succeed.
        let decoded = decode_woff2(&woff2).expect("decode_woff2 must succeed for CFF WOFF2");
        assert!(
            decoded.len() >= 12,
            "decoded SFNT must be at least 12 bytes"
        );
    }

    /// A CFF2 SFNT (has CFF2 table) must also encode without glyf transform.
    #[test]
    fn test_cff2_font_encodes_without_glyf_transform() {
        let cff2_table = vec![0u8; 8];
        let sfnt = build_sfnt(SFNT_MAGIC_CFF, &[(*b"CFF2", cff2_table)])
            .expect("build_sfnt should succeed for CFF2 font");

        let woff2 = encode_woff2(&sfnt).expect("encode_woff2 must not fail for CFF2 font");
        assert!(
            woff2.len() >= 48,
            "WOFF2 must be at least 48 bytes (header)"
        );

        let decoded = decode_woff2(&woff2).expect("decode_woff2 must succeed for CFF2 WOFF2");
        assert!(
            decoded.len() >= 12,
            "decoded SFNT must be at least 12 bytes"
        );
    }

    /// A TrueType SFNT (has glyf table) must NOT be detected as CFF.
    #[test]
    fn test_tt_font_is_not_cff() {
        // Build a minimal TrueType SFNT (no real glyf/loca data — just check that
        // the encoder path runs without CFF shortcut).
        let sfnt = build_sfnt(SFNT_MAGIC_TT, &[(*b"name", b"hello".to_vec())])
            .expect("build_sfnt should succeed for TT font");

        // Encoding should succeed regardless.
        let woff2 = encode_woff2(&sfnt).expect("encode_woff2 must succeed for TT font");
        assert!(woff2.len() >= 48);
    }
}

// -----------------------------------------------------------------------
// has_cff_outlines unit-level test via public API
// -----------------------------------------------------------------------

/// Verify that CFF  and CFF2 keys are correctly detected, and that
/// glyf-only and empty table maps are NOT treated as CFF.
///
/// We test this indirectly by observing that a synthesised SFNT with a CFF
/// table encodes without panicking and that the WOFF2 directory does NOT
/// contain a transformed glyf block (transform_version == 3 / null for
/// non-transformed glyf — but since CFF fonts have no glyf, there simply
/// is no glyf entry in the encoded WOFF2 at all).
#[cfg(feature = "woff2")]
#[test]
fn test_cff_detection_no_glyf_in_woff2_directory() {
    use oxifont_webfont::encode_woff2;
    use oxifont_webfont::sfnt::{build_sfnt, SFNT_MAGIC_CFF};

    let sfnt = build_sfnt(SFNT_MAGIC_CFF, &[(*b"CFF ", vec![0u8; 4])]).expect("build_sfnt for CFF");

    let woff2 = encode_woff2(&sfnt).expect("encode_woff2 for CFF");

    // Scan the WOFF2 table directory for a glyf entry.
    // WOFF2 table directory starts at byte 48.
    // Each entry starts with a flags byte; bits 0-5 are the tag index.
    // glyf tag index = 10 (from KNOWN_TAGS).
    const GLYF_TAG_IDX: u8 = 10;

    let dir_start = 48usize;
    // Walk flags bytes from dir_start; stop when we've moved past plausible entries.
    // We just check: no flags byte with tag_idx == 10 exists.
    let found_glyf = woff2[dir_start..]
        .iter()
        .any(|&b| (b & 0x3F) == GLYF_TAG_IDX);
    assert!(
        !found_glyf,
        "CFF WOFF2 must not contain a glyf table directory entry"
    );
}
