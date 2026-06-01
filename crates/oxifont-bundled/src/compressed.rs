//! Runtime decompression of bundled font bytes.
//!
//! When the `compressed` feature is enabled, bundled font bytes may be stored
//! as zlib/DEFLATE-compressed data and decompressed on first access. When the
//! feature is disabled, this module exposes an identity pass-through that
//! simply copies the raw bytes into an owned `Vec<u8>`.
//!
//! # Note on compressed storage
//!
//! The `compressed` feature adds the decompression API, but the actual
//! build-script step that compresses the font data at compile time is
//! **deferred** (future work). Until that build step is implemented, the
//! data stored in the binary via `include_bytes!` remains uncompressed TTF/OTF,
//! and `decompress_font` is effectively a copy. The feature flag is provided
//! now so that downstream users can opt in and the API is stable when the
//! build script lands.

use oxifont_core::FontError;

/// Decompress a zlib/DEFLATE-compressed font at runtime.
///
/// Returns the decompressed bytes, or [`FontError::ParseError`] if the data
/// is not valid zlib-wrapped DEFLATE.
///
/// # Forward-compatible magic detection
///
/// When the companion build script that compresses font data has not yet run,
/// bundled bytes are stored as raw TTF/OTF. This function detects that case by
/// checking for known SFNT magic bytes (`0x00 0x01 0x00 0x00`, `OTTO`, `ttcf`)
/// and passes the data through as-is rather than attempting zlib decompression.
/// Once the build script lands and starts emitting zlib-wrapped bytes, those
/// bytes will no longer start with SFNT magic, and this function will
/// decompress them correctly.
#[cfg(feature = "compressed")]
pub fn decompress_font(data: &[u8]) -> Result<Vec<u8>, FontError> {
    if is_raw_sfnt(data) {
        // Build script not yet implemented — data is uncompressed TTF/OTF.
        return Ok(data.to_vec());
    }
    oxiarc_deflate::zlib_decompress(data)
        .map_err(|e| FontError::ParseError(format!("bundled font decompression failed: {e}")))
}

/// Return `true` when `data` starts with a known raw SFNT magic sequence.
///
/// Used to detect that the bundled bytes are uncompressed TTF/OTF (i.e. the
/// build script that would compress them has not yet been implemented).
#[cfg(feature = "compressed")]
fn is_raw_sfnt(data: &[u8]) -> bool {
    if data.len() < 4 {
        return false;
    }
    matches!(
        &data[..4],
        [0x00, 0x01, 0x00, 0x00]   // TrueType
            | b"OTTO"               // OpenType/CFF
            | b"ttcf"               // TTC collection
            | b"wOFF"               // WOFF1
            | b"wOF2" // WOFF2
    )
}

/// Identity pass-through when the `compressed` feature is disabled.
///
/// The bundled font data is already a raw TTF/OTF byte slice; this function
/// copies it into an owned `Vec<u8>` for a uniform call site.
#[cfg(not(feature = "compressed"))]
pub fn decompress_font(data: &[u8]) -> Result<Vec<u8>, FontError> {
    Ok(data.to_vec())
}
