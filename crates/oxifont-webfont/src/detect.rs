//! Font format detection and auto-decoding.
//!
//! Provides [`detect_format`] to identify a font file's format from its first
//! 4 bytes, and [`decode_auto`] to decode any supported font format into an
//! SFNT byte buffer.

use crate::error::WebFontError;

// ------------------------------------------------------------------ constants

/// WOFF1 magic: `wOFF`.
const WOFF1_MAGIC: u32 = 0x774F_4646;
/// WOFF2 magic: `wOF2`.
const WOFF2_MAGIC: u32 = 0x774F_4632;
/// TrueType outline magic.
const SFNT_MAGIC_TT: u32 = 0x0001_0000;
/// CFF/OpenType outline magic: `OTTO`.
const SFNT_MAGIC_CFF: u32 = 0x4F54_544F;
/// Apple TrueType: `true`.
const SFNT_MAGIC_TRUE: u32 = 0x7472_7565;
/// TrueType Collection: `ttcf`.
const SFNT_MAGIC_TTCF: u32 = 0x7474_6366;

// ------------------------------------------------------------------ types

/// Font file format as detected from the first 4 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontFormat {
    /// Raw SFNT (TrueType or OpenType/CFF), passed through unchanged.
    Sfnt,
    /// WOFF version 1 (zlib per-table compression).
    Woff1,
    /// WOFF version 2 (single brotli stream).
    Woff2,
    /// Unrecognised magic bytes.
    Unknown,
}

/// Result of auto-decoding a font in any supported format.
pub struct DecodeResult {
    /// The decoded SFNT bytes.
    pub sfnt: Vec<u8>,
    /// Optional extended-metadata string (WOFF1 metadata XML), if present.
    pub metadata: Option<String>,
}

// ------------------------------------------------------------------ API

/// Detect the font file format from the first 4 bytes of `data`.
///
/// Returns [`FontFormat::Unknown`] if `data` is fewer than 4 bytes or does not
/// match a known magic.
pub fn detect_format(data: &[u8]) -> FontFormat {
    if data.len() < 4 {
        return FontFormat::Unknown;
    }
    let magic = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
    match magic {
        WOFF1_MAGIC => FontFormat::Woff1,
        WOFF2_MAGIC => FontFormat::Woff2,
        SFNT_MAGIC_TT | SFNT_MAGIC_CFF | SFNT_MAGIC_TRUE | SFNT_MAGIC_TTCF => FontFormat::Sfnt,
        _ => FontFormat::Unknown,
    }
}

/// Decode a font file in any supported format into an SFNT byte buffer.
///
/// - SFNT input is passed through unchanged (the `sfnt` field is a copy of the input).
/// - WOFF1/WOFF2 are decoded using the respective decoder (feature-gated).
///
/// # Errors
/// Returns [`WebFontError`] if the format is unrecognised, the required feature
/// is not compiled in, or decoding fails.
pub fn decode_auto(data: &[u8]) -> Result<DecodeResult, WebFontError> {
    match detect_format(data) {
        FontFormat::Woff1 => {
            #[cfg(feature = "woff1")]
            {
                let (sfnt, metadata) = crate::woff1::decode_with_metadata(data)?;
                Ok(DecodeResult { sfnt, metadata })
            }
            #[cfg(not(feature = "woff1"))]
            Err(WebFontError::Unsupported("woff1 feature not enabled"))
        }
        FontFormat::Woff2 => {
            #[cfg(feature = "woff2")]
            {
                let (sfnt, metadata) = crate::woff2::decode_with_metadata(data)?;
                Ok(DecodeResult { sfnt, metadata })
            }
            #[cfg(not(feature = "woff2"))]
            Err(WebFontError::Unsupported("woff2 feature not enabled"))
        }
        FontFormat::Sfnt => Ok(DecodeResult {
            sfnt: data.to_vec(),
            metadata: None,
        }),
        FontFormat::Unknown => Err(WebFontError::InvalidSignature),
    }
}

// ----------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_woff1_magic() {
        let data = [0x77u8, 0x4F, 0x46, 0x46, 0, 0, 0, 0];
        assert_eq!(detect_format(&data), FontFormat::Woff1);
    }

    #[test]
    fn detect_woff2_magic() {
        let data = [0x77u8, 0x4F, 0x46, 0x32, 0, 0, 0, 0];
        assert_eq!(detect_format(&data), FontFormat::Woff2);
    }

    #[test]
    fn detect_sfnt_tt() {
        let data = [0x00u8, 0x01, 0x00, 0x00, 0, 0, 0, 0];
        assert_eq!(detect_format(&data), FontFormat::Sfnt);
    }

    #[test]
    fn detect_sfnt_cff() {
        let data = [0x4Fu8, 0x54, 0x54, 0x4F, 0, 0, 0, 0];
        assert_eq!(detect_format(&data), FontFormat::Sfnt);
    }

    #[test]
    fn detect_empty_slice_is_unknown() {
        assert_eq!(detect_format(&[]), FontFormat::Unknown);
    }

    #[test]
    fn detect_three_bytes_is_unknown() {
        assert_eq!(detect_format(&[0x77, 0x4F, 0x46]), FontFormat::Unknown);
    }

    #[test]
    fn detect_garbage_is_unknown() {
        let data = [0xFFu8, 0xFF, 0xFF, 0xFF];
        assert_eq!(detect_format(&data), FontFormat::Unknown);
    }

    #[test]
    fn decode_auto_sfnt_passthrough() {
        // A minimal SFNT (12 bytes offset table only).
        let sfnt = crate::sfnt::build_sfnt(crate::sfnt::SFNT_MAGIC_TT, &[]).expect("empty SFNT");
        let result = decode_auto(&sfnt).expect("should decode SFNT passthrough");
        assert_eq!(result.sfnt, sfnt);
        assert!(result.metadata.is_none());
    }

    #[test]
    fn decode_auto_unknown_returns_error() {
        let data = [0xFFu8, 0xFF, 0xFF, 0xFF];
        let result = decode_auto(&data);
        assert!(matches!(result, Err(WebFontError::InvalidSignature)));
    }
}
