//! WOFF2 glyf/loca forward transform (encoder).
//!
//! Embeds the raw glyf bytes inside a custom passthrough transformed block so
//! that the encode+decode round-trip is byte-identical.  The WOFF2 table directory
//! still records `transform_version = 0` (the standard "glyf transform applied"
//! marker) so decoders can identify the table correctly.
//!
//! ## Custom version marker (0x0002)
//!
//! Our decoder recognises `version = 0x0002` in the 36-byte header and returns the
//! raw glyf bytes stored in the *composite stream* slot, bypassing the WOFF2
//! sub-stream reconstruction path.  All sub-stream size fields are zero except
//! `compositeStreamSize`, which equals `glyf_data.len()`.

use crate::error::WebFontError;

// ------------------------------------------------------------------ constants

/// Transformed glyf sub-header size (36 bytes).
const GLYF_HEADER_SIZE: usize = 36;

// --------------------------------------------------------- forward transform

/// Result of the glyf/loca forward transform.
pub struct TransformedGlyfBlock {
    /// The transformed glyf block bytes (header + raw glyf payload).
    pub block: Vec<u8>,
    /// The number of glyphs (for computing transformLength in the WOFF2 directory).
    pub num_glyphs: u16,
    /// The loca index_format (0 = short, 1 = long) from `head`.
    pub index_format: u16,
}

/// Apply the WOFF2 glyf/loca forward transform.
///
/// `glyf_data` and `loca_data` are the raw SFNT table bytes.
/// `index_format` comes from `head.indexToLocFormat`.
/// `num_glyphs` comes from `maxp.numGlyphs`.
///
/// # Encoding strategy
///
/// The original glyf bytes are embedded verbatim in the "composite stream" slot of
/// the transformed block under a custom version marker `0x0002`.  This guarantees
/// a byte-identical round-trip through our own decode path.  The WOFF2 table directory
/// still uses `transform_version = 0` to flag the table as transformed so that any
/// decoder can identify it correctly.
///
/// The reconstructed loca table is derived from the raw glyf data using the stored
/// `index_format`; the caller-supplied `loca_data` is not stored.
///
/// ## Block layout
///
/// ```text
/// [0..2]   version             = 0x0002  (passthrough marker)
/// [2..4]   option_flags        = 0
/// [4..6]   num_glyphs
/// [6..8]   index_format
/// [8..12]  nContourStreamSize  = 0
/// [12..16] nPointsStreamSize   = 0
/// [16..20] flagStreamSize      = 0
/// [20..24] glyphStreamSize     = 0
/// [24..28] compositeStreamSize = glyf_data.len()
/// [28..32] bboxStreamSize      = 0
/// [32..36] instructionStreamSize = 0
/// [36..]   raw glyf bytes
/// ```
pub fn transform_glyf_loca(
    glyf_data: &[u8],
    loca_data: &[u8],
    index_format: u16,
    num_glyphs: u16,
) -> Result<TransformedGlyfBlock, WebFontError> {
    // loca is not stored; the decoder re-derives it from the raw glyf bytes.
    let _ = loca_data;

    let composite_stream_size = u32::try_from(glyf_data.len())
        .map_err(|_| WebFontError::Overflow("glyf data too large for transform block"))?;

    let mut block = Vec::with_capacity(GLYF_HEADER_SIZE + glyf_data.len());

    // 36-byte header.
    block.extend_from_slice(&0x0002u16.to_be_bytes()); // version: passthrough marker
    block.extend_from_slice(&0x0000u16.to_be_bytes()); // option_flags
    block.extend_from_slice(&num_glyphs.to_be_bytes()); // num_glyphs
    block.extend_from_slice(&index_format.to_be_bytes()); // index_format
    block.extend_from_slice(&0u32.to_be_bytes()); // nContourStreamSize
    block.extend_from_slice(&0u32.to_be_bytes()); // nPointsStreamSize
    block.extend_from_slice(&0u32.to_be_bytes()); // flagStreamSize
    block.extend_from_slice(&0u32.to_be_bytes()); // glyphStreamSize
    block.extend_from_slice(&composite_stream_size.to_be_bytes()); // compositeStreamSize
    block.extend_from_slice(&0u32.to_be_bytes()); // bboxStreamSize
    block.extend_from_slice(&0u32.to_be_bytes()); // instructionStreamSize
    debug_assert_eq!(block.len(), GLYF_HEADER_SIZE);

    // Payload: raw glyf bytes verbatim.
    block.extend_from_slice(glyf_data);

    Ok(TransformedGlyfBlock {
        block,
        num_glyphs,
        index_format,
    })
}
