#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(clippy::all)]

//! `oxifont-webfont` — Pure Rust WOFF1 and WOFF2 decoder.
//!
//! Decodes WOFF1 (zlib per-table via `oxiarc-deflate`) and WOFF2 (brotli via
//! `oxiarc-brotli`) into an SFNT byte buffer that can be parsed by
//! `oxifont-parser`.
//!
//! # Features
//!
//! - `woff1` — enable WOFF1 decoding (requires `oxiarc-deflate`)
//! - `woff2` — enable WOFF2 decoding (requires `oxiarc-brotli`)
//!
//! # Example
//!
//! ```rust,no_run
//! # #[cfg(feature = "woff1")]
//! # {
//! let woff1_data = std::fs::read("font.woff").unwrap();
//! let sfnt = oxifont_webfont::decode_woff1(&woff1_data).unwrap();
//! let face = oxifont_parser::ParsedFace::parse(sfnt, 0).unwrap();
//! println!("{}", oxifont_core::FontFace::family_name(&face));
//! # }
//! ```

/// Error types for webfont decode operations.
pub mod error;
/// SFNT table assembly helpers.
pub mod sfnt;

/// WOFF1 decoder (requires `woff1` feature).
#[cfg(feature = "woff1")]
pub mod woff1;

/// WOFF2 decoder (requires `woff2` feature).
#[cfg(feature = "woff2")]
pub mod woff2;

pub use error::WebFontError;

/// Decode a WOFF1 file into an SFNT byte buffer.
///
/// The result can be passed directly to [`oxifont_parser::ParsedFace::parse`].
///
/// # Errors
/// Returns [`WebFontError`] on invalid input, unsupported format, or
/// decompression failure.
#[cfg(feature = "woff1")]
pub fn decode_woff1(data: &[u8]) -> Result<Vec<u8>, WebFontError> {
    woff1::decode(data)
}

/// Encode an SFNT byte buffer into a WOFF1 file.
///
/// Each table is zlib-compressed (level 9); if compression does not reduce
/// the size, the table is stored uncompressed.
///
/// # Errors
/// Returns [`WebFontError`] on invalid SFNT input or compression failure.
#[cfg(feature = "woff1")]
pub fn encode_woff1(sfnt_data: &[u8]) -> Result<Vec<u8>, WebFontError> {
    woff1::encode::encode(sfnt_data)
}

/// Decode a WOFF2 file into an SFNT byte buffer.
///
/// The result can be passed directly to [`oxifont_parser::ParsedFace::parse`].
///
/// # Errors
/// Returns [`WebFontError`] on invalid input, unsupported format, brotli
/// decompression failure, or malformed transformed-glyf data.
#[cfg(feature = "woff2")]
pub fn decode_woff2(data: &[u8]) -> Result<Vec<u8>, WebFontError> {
    woff2::decode(data)
}

/// Encode an SFNT byte buffer into a WOFF2 file.
///
/// TrueType fonts (with a `glyf` table) have the glyf/loca forward transform applied.
///
/// # Errors
/// Returns [`WebFontError`] on invalid SFNT input or brotli compression failure.
#[cfg(feature = "woff2")]
pub fn encode_woff2(sfnt_data: &[u8]) -> Result<Vec<u8>, WebFontError> {
    woff2::encode::encode(sfnt_data)
}

/// Extract the private data block from a WOFF2 file, if present.
///
/// Returns `None` if the file has no private data block, if the WOFF2 header
/// is missing or invalid, or if the claimed byte range falls outside `data`.
#[cfg(feature = "woff2")]
pub fn extract_woff2_private_data(data: &[u8]) -> Option<Vec<u8>> {
    woff2::extract_woff2_private_data(data)
}

/// Decode a WOFF2 stream into an SFNT byte buffer from any `impl Read` source.
///
/// Unlike [`decode_woff2`], this function does not require the full WOFF2 file
/// to be loaded into memory. The header and table directory are read directly
/// from `reader`, and the brotli-compressed font data block is streamed through
/// the brotli decompressor. The resulting SFNT output is identical to
/// [`decode_woff2`] on the same data.
///
/// # Errors
/// Returns [`WebFontError`] on I/O errors, invalid WOFF2 format, brotli
/// decompression failure, or malformed transformed-glyf data.
#[cfg(feature = "woff2")]
pub fn decode_woff2_streaming<R: std::io::Read>(reader: R) -> Result<Vec<u8>, WebFontError> {
    woff2::decode_streaming(reader)
}

/// Decode all fonts from a WOFF2 font collection (flavor `ttcf`).
///
/// Returns one SFNT byte buffer per font in the collection.  Returns `Err` if
/// the data is not a valid WOFF2 font collection; use [`decode_woff2`] for
/// single-font WOFF2 files.
///
/// # Errors
/// Returns [`WebFontError`] on invalid input, non-collection flavor, brotli
/// decompression failure, or malformed collection structures.
#[cfg(feature = "woff2")]
pub fn decode_woff2_collection(data: &[u8]) -> Result<Vec<Vec<u8>>, WebFontError> {
    woff2::decode_collection(data)
}

/// Format detection and auto-decoding API.
pub mod detect;
pub use detect::{decode_auto, detect_format, DecodeResult, FontFormat};
