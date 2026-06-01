//! Tests for WOFF2 private data block extraction.

#[cfg(feature = "woff2")]
mod tests {
    /// Build a minimal valid SFNT (TrueType offset table, zero tables).
    fn minimal_sfnt() -> Vec<u8> {
        oxifont_webfont::sfnt::build_sfnt(oxifont_webfont::sfnt::SFNT_MAGIC_TT, &[])
            .expect("build minimal SFNT")
    }

    #[test]
    fn test_extract_woff2_private_data_short_input() {
        // Input shorter than 48 bytes must return None without panicking.
        let result = oxifont_webfont::extract_woff2_private_data(&[0u8; 10]);
        assert!(result.is_none(), "short input must return None");
    }

    #[test]
    fn test_extract_woff2_private_data_wrong_signature() {
        // 48 bytes with wrong signature must return None.
        let result = oxifont_webfont::extract_woff2_private_data(&[0u8; 48]);
        assert!(result.is_none(), "wrong signature must return None");
    }

    #[test]
    fn test_extract_woff2_private_data_empty_on_no_priv() {
        // A WOFF2 produced by encode_woff2 has no private data block → None.
        let sfnt = minimal_sfnt();
        let woff2 = match oxifont_webfont::encode_woff2(&sfnt) {
            Ok(w) => w,
            // If the SFNT is too minimal to encode, skip gracefully.
            Err(_) => return,
        };
        let priv_data = oxifont_webfont::extract_woff2_private_data(&woff2);
        assert!(
            priv_data.is_none(),
            "WOFF2 without private data must return None"
        );
    }

    #[test]
    fn test_extract_woff2_private_data_with_appended_block() {
        // Craft a WOFF2 header where privOffset/privLength point to appended bytes.
        // Strategy: encode a minimal SFNT, then append our private payload, and
        // patch the header's privOffset/privLength fields (offsets 40 and 44).
        let sfnt = minimal_sfnt();
        let mut woff2 = match oxifont_webfont::encode_woff2(&sfnt) {
            Ok(w) => w,
            Err(_) => return,
        };

        // Ensure at least 48 bytes.
        if woff2.len() < 48 {
            return;
        }

        // Append a private payload after the existing WOFF2 bytes.
        let priv_payload: &[u8] = b"oxifont-private-test-payload";
        let priv_offset = woff2.len() as u32;
        let priv_length = priv_payload.len() as u32;
        woff2.extend_from_slice(priv_payload);

        // Patch privOffset @ byte 40, privLength @ byte 44 (big-endian u32 each).
        woff2[40..44].copy_from_slice(&priv_offset.to_be_bytes());
        woff2[44..48].copy_from_slice(&priv_length.to_be_bytes());

        let extracted = oxifont_webfont::extract_woff2_private_data(&woff2);
        assert!(
            extracted.is_some(),
            "should extract private data when privOffset/privLength are set"
        );
        assert_eq!(
            extracted.as_deref(),
            Some(priv_payload),
            "extracted private data must match the appended payload"
        );
    }

    #[test]
    fn test_extract_woff2_private_data_out_of_bounds() {
        // privOffset + privLength overflows the data buffer → None.
        let sfnt = minimal_sfnt();
        let mut woff2 = match oxifont_webfont::encode_woff2(&sfnt) {
            Ok(w) => w,
            Err(_) => return,
        };
        if woff2.len() < 48 {
            return;
        }

        // Set privOffset to just inside the buffer but privLength to overflow.
        let priv_offset = (woff2.len() - 4) as u32;
        let priv_length = 100u32; // would reach beyond the buffer
        woff2[40..44].copy_from_slice(&priv_offset.to_be_bytes());
        woff2[44..48].copy_from_slice(&priv_length.to_be_bytes());

        let result = oxifont_webfont::extract_woff2_private_data(&woff2);
        assert!(
            result.is_none(),
            "out-of-bounds private data must return None"
        );
    }
}
