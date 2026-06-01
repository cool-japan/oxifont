//! WOFF1 decoder and encoder.
//!
//! Parses the WOFF header and table directory, decompresses each table using
//! zlib (via `oxiarc-deflate`), and reassembles a valid SFNT byte buffer.
//!
//! Reference: W3C WOFF 1.0 specification
//! <https://www.w3.org/TR/WOFF/>

/// WOFF1 encoder.
pub mod encode;

use oxiarc_deflate::zlib_decompress;

use crate::error::WebFontError;
use crate::sfnt::{build_sfnt, detect_sfnt_version, table_checksum};

// ---------------------------------------------------------------- constants

/// WOFF1 signature: `wOFF` = 0x774F4646.
const WOFF1_SIGNATURE: u32 = 0x774F_4646;

/// Minimum WOFF1 header size (44 bytes).
const WOFF1_HEADER_SIZE: usize = 44;

/// Table directory entry size (20 bytes per entry).
const WOFF1_DIR_ENTRY_SIZE: usize = 20;

// --------------------------------------------------------------- structures

/// Parsed WOFF1 header fields.
struct Woff1Header {
    /// SFNT version of the wrapped font.
    sf_version: u32,
    /// Number of tables.
    num_tables: u16,
    /// Total size of the WOFF data (we validate against actual slice length).
    _total_sfnt_size: u32,
    /// Byte offset of the metadata block within the WOFF file (0 = absent).
    meta_offset: u32,
    /// Compressed length of the metadata block in bytes (0 = absent).
    meta_length: u32,
}

/// A single WOFF1 table directory entry.
struct Woff1TableEntry {
    tag: [u8; 4],
    offset: u32,
    comp_length: u32,
    orig_length: u32,
    orig_checksum: u32,
}

// ------------------------------------------------------------- reader helpers

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

// ---------------------------------------------------------------- parser

fn parse_header(data: &[u8]) -> Result<Woff1Header, WebFontError> {
    if data.len() < WOFF1_HEADER_SIZE {
        return Err(WebFontError::TooShort);
    }

    let signature = read_u32_be(data, 0)?;
    if signature != WOFF1_SIGNATURE {
        return Err(WebFontError::InvalidSignature);
    }

    let sf_version = read_u32_be(data, 4)?;
    // Offset 8: length (total WOFF file length).
    let _length = read_u32_be(data, 8)?;
    let num_tables = read_u16_be(data, 12)?;
    // Offset 14: reserved (must be 0).
    let reserved = read_u16_be(data, 14)?;
    if reserved != 0 {
        return Err(WebFontError::InvalidField {
            field: "reserved",
            value: reserved as u64,
        });
    }
    let total_sfnt_size = read_u32_be(data, 16)?;
    // Offsets 20–23: majorVersion, minorVersion (ignored for decoding)
    // Offset 24: metaOffset (u32), Offset 28: metaLength (u32)
    let meta_offset = read_u32_be(data, 24)?;
    let meta_length = read_u32_be(data, 28)?;
    // Offsets 32–43: metaOrigLength, privOffset, privLength (ignored)

    Ok(Woff1Header {
        sf_version,
        num_tables,
        _total_sfnt_size: total_sfnt_size,
        meta_offset,
        meta_length,
    })
}

fn parse_table_entry(data: &[u8], offset: usize) -> Result<Woff1TableEntry, WebFontError> {
    if data.len() < offset + WOFF1_DIR_ENTRY_SIZE {
        return Err(WebFontError::TooShort);
    }

    let tag: [u8; 4] = data[offset..offset + 4]
        .try_into()
        .map_err(|_| WebFontError::TooShort)?;
    let entry_offset = read_u32_be(data, offset + 4)?;
    let comp_length = read_u32_be(data, offset + 8)?;
    let orig_length = read_u32_be(data, offset + 12)?;
    let orig_checksum = read_u32_be(data, offset + 16)?;

    Ok(Woff1TableEntry {
        tag,
        offset: entry_offset,
        comp_length,
        orig_length,
        orig_checksum,
    })
}

// ------------------------------------------------------------------ decoder

/// Decode a WOFF1 file into an SFNT byte buffer together with optional
/// extended metadata XML (if the WOFF1 file contains a metadata block).
///
/// The metadata block, when present, is zlib-decompressed and returned as a
/// UTF-8 string. Lossy conversion is used for byte sequences that are not
/// valid UTF-8.
///
/// This is the low-level variant used by [`crate::detect::decode_auto`].
pub fn decode_with_metadata(data: &[u8]) -> Result<(Vec<u8>, Option<String>), WebFontError> {
    let header = parse_header(data)?;

    let num_tables = header.num_tables as usize;
    let dir_start = WOFF1_HEADER_SIZE;

    // Parse table directory.
    let mut tables: Vec<([u8; 4], Vec<u8>)> = Vec::with_capacity(num_tables);

    for i in 0..num_tables {
        let entry_offset = dir_start + i * WOFF1_DIR_ENTRY_SIZE;
        let entry = parse_table_entry(data, entry_offset)?;

        // Bounds-check the table data region.
        let start = entry.offset as usize;
        let end = start
            .checked_add(entry.comp_length as usize)
            .ok_or(WebFontError::Overflow("table end offset"))?;
        let table_data = data.get(start..end).ok_or(WebFontError::OutOfBounds {
            context: "table data",
        })?;

        // Decompress or copy.
        // For the uncompressed path we pre-allocate the output buffer using
        // orig_length from the table directory entry (avoids a realloc for
        // large tables). For the compressed path the zlib decompressor manages
        // its own internal buffer and returns an owned Vec; we cannot supply a
        // capacity hint to it via the current oxiarc-deflate API.
        let decompressed: Vec<u8> = if entry.comp_length < entry.orig_length {
            // Compressed — use zlib.
            zlib_decompress(table_data).map_err(|e| WebFontError::DecompressError(e.to_string()))?
        } else {
            // Uncompressed — pre-allocate exact capacity then copy.
            let mut buf = Vec::with_capacity(entry.orig_length as usize);
            buf.extend_from_slice(table_data);
            buf
        };

        // Verify decompressed size.
        if decompressed.len() != entry.orig_length as usize {
            return Err(WebFontError::LengthMismatch {
                tag: entry.tag,
                expected: entry.orig_length,
                got: decompressed.len(),
            });
        }

        // Verify checksum (skip head table — its checkSumAdjustment is
        // intentionally wrong at this point; it will be fixed by build_sfnt).
        if entry.tag != *b"head" {
            let actual_checksum = table_checksum(&decompressed);
            if actual_checksum != entry.orig_checksum {
                return Err(WebFontError::ChecksumMismatch { tag: entry.tag });
            }
        }

        tables.push((entry.tag, decompressed));
    }

    // Sort tables by tag for maximum interoperability.
    tables.sort_by_key(|(tag, _)| *tag);

    // Determine sfntVersion from the WOFF header's sfVersion field.
    // 0x0001_0000 = TrueType, 0x4F54_544F ("OTTO") = CFF/OpenType.
    // Fall back to table-based detection if the sfVersion is not one of these.
    let sfnt_version = if header.sf_version == 0x0001_0000 || header.sf_version == 0x4F54_544F {
        header.sf_version
    } else {
        detect_sfnt_version(&tables)
    };

    let sfnt = build_sfnt(sfnt_version, &tables)?;

    // Extract metadata block (zlib-compressed XML, WOFF1 spec §6).
    let metadata = extract_woff1_metadata(data, header.meta_offset, header.meta_length)?;

    Ok((sfnt, metadata))
}

/// Extract and decompress the WOFF1 metadata block, if present.
fn extract_woff1_metadata(
    data: &[u8],
    meta_offset: u32,
    meta_length: u32,
) -> Result<Option<String>, WebFontError> {
    if meta_offset == 0 || meta_length == 0 {
        return Ok(None);
    }

    let start = meta_offset as usize;
    let end = start
        .checked_add(meta_length as usize)
        .ok_or(WebFontError::Overflow("metadata block end"))?;
    let compressed = data.get(start..end).ok_or(WebFontError::OutOfBounds {
        context: "WOFF1 metadata block",
    })?;

    let decompressed = zlib_decompress(compressed)
        .map_err(|e| WebFontError::DecompressError(format!("metadata: {e}")))?;

    Ok(Some(String::from_utf8_lossy(&decompressed).into_owned()))
}

/// Decode a WOFF1 file into an SFNT byte buffer.
pub fn decode(data: &[u8]) -> Result<Vec<u8>, WebFontError> {
    decode_with_metadata(data).map(|(sfnt, _)| sfnt)
}

// ----------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_rejects_bad_signature() {
        let mut bad = vec![0u8; 48];
        // Signature bytes wrong.
        bad[0] = 0xFF;
        let result = decode(&bad);
        assert!(matches!(result, Err(WebFontError::InvalidSignature)));
    }

    #[test]
    fn decode_rejects_short_data() {
        let short = vec![0u8; 10];
        let result = decode(&short);
        assert!(matches!(result, Err(WebFontError::TooShort)));
    }
}
