//! Runtime decompression of bundled font bytes.
//!
//! When the `compressed` feature is enabled, bundled font bytes in
//! [`BundledFont`](crate::catalog::BundledFont) are stored as
//! zlib/DEFLATE-compressed data (produced by `build.rs` at compile time) and
//! decompressed on first access.  When the feature is disabled, this module
//! exposes an identity pass-through that copies the raw bytes into an owned
//! `Vec<u8>`.

use oxifont_core::FontError;

/// Decompress a zlib/DEFLATE-compressed font at runtime.
///
/// Returns the decompressed bytes, or [`FontError::ParseError`] if the data
/// is not valid zlib-wrapped DEFLATE.
///
/// The `build.rs` script always writes properly compressed output, so every
/// `&[u8]` stored in a [`BundledFont`](crate::catalog::BundledFont) under the
/// `compressed` feature is a real zlib stream.
#[cfg(feature = "compressed")]
pub fn decompress_font(data: &[u8]) -> Result<Vec<u8>, FontError> {
    oxiarc_deflate::zlib_decompress(data)
        .map_err(|e| FontError::ParseError(format!("bundled font decompression failed: {e}")))
}

/// Identity pass-through when the `compressed` feature is disabled.
///
/// The bundled font data is already a raw TTF/OTF byte slice; this function
/// copies it into an owned `Vec<u8>` for a uniform call site.
#[cfg(not(feature = "compressed"))]
pub fn decompress_font(data: &[u8]) -> Result<Vec<u8>, FontError> {
    Ok(data.to_vec())
}
