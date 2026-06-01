//! WOFF2 encoder.
//!
//! Encodes a valid SFNT (TrueType/OpenType) byte buffer into a WOFF2 file.
//! TrueType fonts (with a `glyf` table) have the glyf/loca forward transform
//! applied; CFF/other fonts use null transform for all tables.
//!
//! Reference: W3C WOFF2 specification
//! <https://www.w3.org/TR/WOFF2/>

/// WOFF2 glyf/loca forward transform (encoder side).
pub mod glyf;
/// Variable-length integer encoders for WOFF2.
pub mod varint;

use oxiarc_brotli::compress;

use crate::error::WebFontError;
use crate::woff2::header::{KNOWN_TAGS, WOFF2_SIGNATURE};

use self::glyf::transform_glyf_loca;
use self::varint::encode_uint_base128;

// ------------------------------------------------------------------ constants

/// WOFF2 header size (48 bytes).
const WOFF2_HEADER_SIZE: usize = 48;

/// SFNT offset table size (12 bytes).
const SFNT_OFFSET_TABLE_SIZE: usize = 12;

/// SFNT table directory entry size (16 bytes).
const SFNT_DIR_ENTRY_SIZE: usize = 16;

// ------------------------------------------------------------------ helpers

fn read_u16_be(data: &[u8], offset: usize) -> Result<u16, WebFontError> {
    data.get(offset..offset + 2)
        .ok_or(WebFontError::TooShort)
        .map(|b| u16::from_be_bytes([b[0], b[1]]))
}

fn read_u32_be(data: &[u8], offset: usize) -> Result<u32, WebFontError> {
    data.get(offset..offset + 4)
        .ok_or(WebFontError::TooShort)
        .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

// --------------------------------------------------------------- SFNT parser

/// A single table extracted from an SFNT.
struct SfntTable {
    tag: [u8; 4],
    data: Vec<u8>,
}

/// Parse the SFNT offset table and extract all tables.
fn parse_sfnt_tables(data: &[u8]) -> Result<(u32, Vec<SfntTable>), WebFontError> {
    if data.len() < SFNT_OFFSET_TABLE_SIZE {
        return Err(WebFontError::TooShort);
    }

    let sfnt_version = read_u32_be(data, 0)?;
    let num_tables = read_u16_be(data, 4)? as usize;

    let dir_end = SFNT_OFFSET_TABLE_SIZE + num_tables * SFNT_DIR_ENTRY_SIZE;
    if data.len() < dir_end {
        return Err(WebFontError::TooShort);
    }

    let mut tables = Vec::with_capacity(num_tables);
    for i in 0..num_tables {
        let base = SFNT_OFFSET_TABLE_SIZE + i * SFNT_DIR_ENTRY_SIZE;
        let tag: [u8; 4] = data[base..base + 4]
            .try_into()
            .map_err(|_| WebFontError::TooShort)?;
        let offset = read_u32_be(data, base + 8)? as usize;
        let length = read_u32_be(data, base + 12)? as usize;

        let end = offset
            .checked_add(length)
            .ok_or(WebFontError::Overflow("table end"))?;
        let table_data = data.get(offset..end).ok_or(WebFontError::OutOfBounds {
            context: "SFNT table data",
        })?;

        tables.push(SfntTable {
            tag,
            data: table_data.to_vec(),
        });
    }

    Ok((sfnt_version, tables))
}

// --------------------------------------------------------- tag-index lookup

/// Find the WOFF2 KNOWN_TAGS index for a 4-byte tag, or None if unknown.
fn known_tag_index(tag: &[u8; 4]) -> Option<u8> {
    KNOWN_TAGS.iter().position(|k| *k == tag).map(|i| i as u8)
}

// --------------------------------------------------------- needs_transform_length

/// Returns true when a separate `transformLength` field must be emitted for this
/// (tag, transform_version) combination in the WOFF2 table directory.
fn needs_transform_length(tag: [u8; 4], transform_version: u8) -> bool {
    matches!(
        (&tag, transform_version),
        (b"glyf", 0) | (b"loca", 0) | (b"hmtx", 1)
    )
}

// --------------------------------------------------------- totalSfntSize

/// Compute the `totalSfntSize` field: the size of the uncompressed SFNT.
fn compute_total_sfnt_size(tables: &[SfntTable]) -> Result<u32, WebFontError> {
    let dir_size = SFNT_OFFSET_TABLE_SIZE + tables.len() * SFNT_DIR_ENTRY_SIZE;
    let mut total = dir_size as u64;
    for t in tables {
        let padded = ((t.data.len() as u64) + 3) & !3;
        total = total
            .checked_add(padded)
            .ok_or(WebFontError::Overflow("totalSfntSize"))?;
    }
    u32::try_from(total).map_err(|_| WebFontError::Overflow("totalSfntSize u32"))
}

// ---------------------------------------------------------- directory writer

/// One entry as it will appear in the WOFF2 table directory.
struct DirEntry {
    tag: [u8; 4],
    transform_version: u8,
    orig_length: u32,
    /// `Some(len)` when `needs_transform_length` is true; `None` otherwise.
    transform_length: Option<u32>,
}

/// Serialise one WOFF2 table directory entry into `out`.
fn write_dir_entry(out: &mut Vec<u8>, entry: &DirEntry) {
    let tag_idx = known_tag_index(&entry.tag);
    let flags_byte = if let Some(idx) = tag_idx {
        (entry.transform_version << 6) | idx
    } else {
        (entry.transform_version << 6) | 63u8
    };
    out.push(flags_byte);
    if tag_idx.is_none() {
        out.extend_from_slice(&entry.tag);
    }
    encode_uint_base128(out, entry.orig_length);
    if let Some(tl) = entry.transform_length {
        encode_uint_base128(out, tl);
    }
}

// ------------------------------------------------------------------ CFF detection

/// Returns `true` when the table set contains a CFF or CFF2 outline table.
///
/// When CFF outlines are present, the glyf/loca forward transform must NOT be
/// applied. All tables (including any glyf/loca stubs, which are absent in
/// well-formed CFF fonts) are stored with null transform.
fn has_cff_outlines(tables: &[SfntTable]) -> bool {
    tables
        .iter()
        .any(|t| &t.tag == b"CFF " || &t.tag == b"CFF2")
}

// ------------------------------------------------------------------ encoder

/// Encode an SFNT byte buffer into a WOFF2 file.
///
/// # Errors
/// Returns [`WebFontError`] on invalid SFNT input or brotli compression failure.
pub fn encode(sfnt_data: &[u8]) -> Result<Vec<u8>, WebFontError> {
    let (sfnt_version, mut tables) = parse_sfnt_tables(sfnt_data)?;

    // Sort by tag for canonical order.
    tables.sort_by_key(|t| t.tag);

    let total_sfnt_size = compute_total_sfnt_size(&tables)?;

    // Detect TrueType (has glyf table) — but only apply glyf transform when
    // the font does NOT use CFF/CFF2 outlines. CFF fonts must use null transform.
    let cff_font = has_cff_outlines(&tables);
    let has_glyf = tables.iter().any(|t| &t.tag == b"glyf");
    let has_loca = tables.iter().any(|t| &t.tag == b"loca");

    // We need head (for indexToLocFormat) and maxp (for numGlyphs) to apply glyf transform.
    let (index_format, num_glyphs): (u16, u16) = if has_glyf {
        let head_data = tables
            .iter()
            .find(|t| &t.tag == b"head")
            .map(|t| t.data.as_slice());
        let maxp_data = tables
            .iter()
            .find(|t| &t.tag == b"maxp")
            .map(|t| t.data.as_slice());

        let idx_fmt = head_data
            .and_then(|d| d.get(50..52))
            .map(|b| u16::from_be_bytes([b[0], b[1]]))
            .unwrap_or(0);
        let ng = maxp_data
            .and_then(|d| d.get(4..6))
            .map(|b| u16::from_be_bytes([b[0], b[1]]))
            .unwrap_or(0);
        (idx_fmt, ng)
    } else {
        (0, 0)
    };

    // Build transformed glyf block if needed — only for TrueType (non-CFF) fonts.
    let transformed_glyf = if has_glyf && has_loca && !cff_font {
        let glyf_data = tables
            .iter()
            .find(|t| &t.tag == b"glyf")
            .map(|t| t.data.as_slice())
            .ok_or(WebFontError::OutOfBounds { context: "glyf" })?;
        let loca_data = tables
            .iter()
            .find(|t| &t.tag == b"loca")
            .map(|t| t.data.as_slice())
            .ok_or(WebFontError::OutOfBounds { context: "loca" })?;
        Some(transform_glyf_loca(
            glyf_data,
            loca_data,
            index_format,
            num_glyphs,
        )?)
    } else {
        None
    };

    // Build directory entries and the concatenated font data stream.
    let mut dir_entries: Vec<DirEntry> = Vec::with_capacity(tables.len());
    let mut font_data_stream: Vec<u8> = Vec::new();

    for t in &tables {
        if &t.tag == b"loca" && transformed_glyf.is_some() {
            // Transformed loca: transform_version=0, transformLength=0, no data in stream.
            let orig_length = t.data.len() as u32;
            dir_entries.push(DirEntry {
                tag: *b"loca",
                transform_version: 0,
                orig_length,
                transform_length: Some(0), // transformLength = 0 means reconstructed from glyf
            });
            continue;
        }

        if &t.tag == b"glyf" {
            if let Some(ref tglyf) = transformed_glyf {
                // Transformed glyf: transform_version=0, transformLength = transformed block size.
                let orig_length = t.data.len() as u32;
                let transform_length = tglyf.block.len() as u32;
                dir_entries.push(DirEntry {
                    tag: *b"glyf",
                    transform_version: 0,
                    orig_length,
                    transform_length: Some(transform_length),
                });
                font_data_stream.extend_from_slice(&tglyf.block);
                continue;
            }
        }

        // All other tables: null transform (transform_version=0).
        // For glyf/loca without transform: transform_version=3 (null, per spec §5.1).
        let is_glyf_or_loca = &t.tag == b"glyf" || &t.tag == b"loca";
        let transform_version = if is_glyf_or_loca { 3u8 } else { 0u8 };
        let orig_length = t.data.len() as u32;
        let transform_length = if needs_transform_length(t.tag, transform_version) {
            Some(orig_length)
        } else {
            None
        };

        dir_entries.push(DirEntry {
            tag: t.tag,
            transform_version,
            orig_length,
            transform_length,
        });
        font_data_stream.extend_from_slice(&t.data);
    }

    // Brotli-compress the font data stream.
    let compressed = compress(&font_data_stream, 11)
        .map_err(|e| WebFontError::DecompressError(e.to_string()))?;

    let total_compressed_size = compressed.len() as u32;

    // Serialise the table directory.
    let mut table_dir: Vec<u8> = Vec::new();
    for entry in &dir_entries {
        write_dir_entry(&mut table_dir, entry);
    }

    // Compute total WOFF2 file length.
    let total_length = WOFF2_HEADER_SIZE
        .checked_add(table_dir.len())
        .and_then(|s| s.checked_add(compressed.len()))
        .ok_or(WebFontError::Overflow("WOFF2 total length"))?;
    let total_length_u32 = u32::try_from(total_length)
        .map_err(|_| WebFontError::Overflow("WOFF2 total length u32"))?;

    let num_tables = tables.len() as u16;

    // Build output.
    let mut out = Vec::with_capacity(total_length);

    // ---- WOFF2 header (48 bytes) --------------------------------------------
    out.extend_from_slice(&WOFF2_SIGNATURE.to_be_bytes()); // signature
    out.extend_from_slice(&sfnt_version.to_be_bytes()); // flavor
    out.extend_from_slice(&total_length_u32.to_be_bytes()); // length
    out.extend_from_slice(&num_tables.to_be_bytes()); // numTables
    out.extend_from_slice(&0u16.to_be_bytes()); // reserved
    out.extend_from_slice(&total_sfnt_size.to_be_bytes()); // totalSfntSize
    out.extend_from_slice(&total_compressed_size.to_be_bytes()); // totalCompressedSize
    out.extend_from_slice(&0u16.to_be_bytes()); // majorVersion
    out.extend_from_slice(&0u16.to_be_bytes()); // minorVersion
    out.extend_from_slice(&0u32.to_be_bytes()); // metaOffset
    out.extend_from_slice(&0u32.to_be_bytes()); // metaLength
    out.extend_from_slice(&0u32.to_be_bytes()); // metaOrigLength
    out.extend_from_slice(&0u32.to_be_bytes()); // privOffset
    out.extend_from_slice(&0u32.to_be_bytes()); // privLength
    debug_assert_eq!(out.len(), WOFF2_HEADER_SIZE);

    // ---- Table directory ---------------------------------------------------
    out.extend(table_dir);

    // ---- Compressed font data block ----------------------------------------
    out.extend(compressed);

    Ok(out)
}

// ----------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sfnt::{build_sfnt, SFNT_MAGIC_TT};

    #[test]
    fn encode_rejects_too_short() {
        let result = encode(&[0u8; 4]);
        assert!(matches!(result, Err(WebFontError::TooShort)));
    }

    #[test]
    fn known_tag_index_glyf() {
        let idx = known_tag_index(b"glyf");
        assert!(idx.is_some());
        assert_eq!(KNOWN_TAGS[idx.expect("glyf known") as usize], b"glyf");
    }

    #[test]
    fn known_tag_index_unknown() {
        let idx = known_tag_index(b"XXXX");
        assert!(idx.is_none());
    }

    #[test]
    fn encode_empty_sfnt() {
        let sfnt = build_sfnt(SFNT_MAGIC_TT, &[]).expect("empty SFNT");
        let woff2 = encode(&sfnt).expect("encode should succeed");
        assert!(woff2.len() >= WOFF2_HEADER_SIZE);
    }
}
