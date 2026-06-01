//! WOFF1 encoder.
//!
//! Encodes a valid SFNT (TrueType/OpenType) byte buffer into a WOFF1 file using
//! per-table zlib deflate compression.
//!
//! Reference: W3C WOFF 1.0 specification
//! <https://www.w3.org/TR/WOFF/>

use oxiarc_deflate::zlib_compress;

use crate::error::WebFontError;
use crate::sfnt::table_checksum;

// ------------------------------------------------------------------ constants

/// WOFF1 signature: `wOFF` = 0x774F4646.
const WOFF1_SIGNATURE: u32 = 0x774F_4646;

/// WOFF1 header size (44 bytes).
const WOFF1_HEADER_SIZE: usize = 44;

/// WOFF1 table directory entry size (20 bytes).
const WOFF1_DIR_ENTRY_SIZE: usize = 20;

/// SFNT offset table size (12 bytes).
const SFNT_OFFSET_TABLE_SIZE: usize = 12;

/// SFNT table directory entry size (16 bytes: tag + checksum + offset + length).
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

/// A parsed SFNT table entry.
struct SfntTableEntry {
    tag: [u8; 4],
    checksum: u32,
    offset: usize,
    length: usize,
}

/// Parse the SFNT offset table + table directory to extract table metadata.
fn parse_sfnt(data: &[u8]) -> Result<(u32, Vec<SfntTableEntry>), WebFontError> {
    if data.len() < SFNT_OFFSET_TABLE_SIZE {
        return Err(WebFontError::TooShort);
    }

    let sfnt_version = read_u32_be(data, 0)?;
    let num_tables = read_u16_be(data, 4)? as usize;

    let dir_end = SFNT_OFFSET_TABLE_SIZE + num_tables * SFNT_DIR_ENTRY_SIZE;
    if data.len() < dir_end {
        return Err(WebFontError::TooShort);
    }

    let mut entries = Vec::with_capacity(num_tables);
    for i in 0..num_tables {
        let base = SFNT_OFFSET_TABLE_SIZE + i * SFNT_DIR_ENTRY_SIZE;
        let tag_bytes = &data[base..base + 4];
        let tag: [u8; 4] = tag_bytes.try_into().map_err(|_| WebFontError::TooShort)?;
        let checksum = read_u32_be(data, base + 4)?;
        let offset = read_u32_be(data, base + 8)? as usize;
        let length = read_u32_be(data, base + 12)? as usize;
        entries.push(SfntTableEntry {
            tag,
            checksum,
            offset,
            length,
        });
    }

    Ok((sfnt_version, entries))
}

// ------------------------------------------------------------------ encoder

/// Encode an SFNT byte buffer into a WOFF1 file.
///
/// Each table is zlib-compressed; if compression does not reduce size, the
/// table is stored uncompressed (WOFF1 spec §5.3).
///
/// # Errors
/// Returns [`WebFontError`] on invalid SFNT input or compression failure.
pub fn encode(sfnt_data: &[u8]) -> Result<Vec<u8>, WebFontError> {
    let (sfnt_version, mut entries) = parse_sfnt(sfnt_data)?;

    // Sort tables by tag — required for interoperability and round-trip fidelity.
    entries.sort_by_key(|e| e.tag);

    let num_tables = entries.len();

    // For each table: extract raw bytes, compress, decide.
    struct CompressedTable {
        tag: [u8; 4],
        orig_checksum: u32,
        orig_length: u32,
        data: Vec<u8>, // compressed or raw (whichever is shorter)
    }

    let mut compressed_tables: Vec<CompressedTable> = Vec::with_capacity(num_tables);

    for entry in &entries {
        let end = entry
            .offset
            .checked_add(entry.length)
            .ok_or(WebFontError::Overflow("table end offset"))?;
        let raw = sfnt_data
            .get(entry.offset..end)
            .ok_or(WebFontError::OutOfBounds {
                context: "SFNT table data",
            })?;

        // Recompute checksum from raw data (the SFNT checksum may not match if it
        // was assembled without head fixup; use the stored value for `head`).
        let orig_checksum = if entry.tag == *b"head" {
            entry.checksum
        } else {
            table_checksum(raw)
        };

        let compressed =
            zlib_compress(raw, 9).map_err(|e| WebFontError::DecompressError(e.to_string()))?;

        let data = if compressed.len() < raw.len() {
            compressed
        } else {
            raw.to_vec()
        };

        compressed_tables.push(CompressedTable {
            tag: entry.tag,
            orig_checksum,
            orig_length: entry.length as u32,
            data,
        });
    }

    // Compute the totalSfntSize: 12 (offset table) + 16 × numTables + sum of ceil4(origLength).
    let total_sfnt_size: u64 = {
        let mut s = (SFNT_OFFSET_TABLE_SIZE + num_tables * SFNT_DIR_ENTRY_SIZE) as u64;
        for ct in &compressed_tables {
            let padded = ((ct.orig_length as u64) + 3) & !3;
            s = s
                .checked_add(padded)
                .ok_or(WebFontError::Overflow("totalSfntSize"))?;
        }
        s
    };
    let total_sfnt_size =
        u32::try_from(total_sfnt_size).map_err(|_| WebFontError::Overflow("totalSfntSize u32"))?;

    // Compute per-table offsets in the WOFF file.
    // Tables start immediately after header + directory.
    let data_start = WOFF1_HEADER_SIZE + num_tables * WOFF1_DIR_ENTRY_SIZE;

    // Assign offsets.
    struct TableWithOffset {
        tag: [u8; 4],
        woff_offset: u32,
        comp_length: u32,
        orig_length: u32,
        orig_checksum: u32,
        data: Vec<u8>,
    }

    let mut tables_with_offsets: Vec<TableWithOffset> = Vec::with_capacity(num_tables);
    let mut current_offset = data_start;

    for ct in compressed_tables {
        let comp_length = ct.data.len() as u32;
        let woff_offset = u32::try_from(current_offset)
            .map_err(|_| WebFontError::Overflow("WOFF table offset"))?;

        // Pad to 4-byte alignment for the next table.
        let padded = (ct.data.len() + 3) & !3;
        current_offset = current_offset
            .checked_add(padded)
            .ok_or(WebFontError::Overflow("WOFF running offset"))?;

        tables_with_offsets.push(TableWithOffset {
            tag: ct.tag,
            woff_offset,
            comp_length,
            orig_length: ct.orig_length,
            orig_checksum: ct.orig_checksum,
            data: ct.data,
        });
    }

    let total_length =
        u32::try_from(current_offset).map_err(|_| WebFontError::Overflow("WOFF total length"))?;

    // Build WOFF output.
    let mut out = Vec::with_capacity(current_offset);

    // ---- Write WOFF header (44 bytes) ----------------------------------------
    out.extend_from_slice(&WOFF1_SIGNATURE.to_be_bytes()); // signature
    out.extend_from_slice(&sfnt_version.to_be_bytes()); // flavor
    out.extend_from_slice(&total_length.to_be_bytes()); // length
    out.extend_from_slice(&(num_tables as u16).to_be_bytes()); // numTables
    out.extend_from_slice(&0u16.to_be_bytes()); // reserved
    out.extend_from_slice(&total_sfnt_size.to_be_bytes()); // totalSfntSize
    out.extend_from_slice(&0u16.to_be_bytes()); // majorVersion
    out.extend_from_slice(&0u16.to_be_bytes()); // minorVersion
    out.extend_from_slice(&0u32.to_be_bytes()); // metaOffset
    out.extend_from_slice(&0u32.to_be_bytes()); // metaLength
    out.extend_from_slice(&0u32.to_be_bytes()); // metaOrigLength
    out.extend_from_slice(&0u32.to_be_bytes()); // privOffset
    out.extend_from_slice(&0u32.to_be_bytes()); // privLength
    debug_assert_eq!(out.len(), WOFF1_HEADER_SIZE);

    // ---- Write table directory (20 bytes × numTables) ------------------------
    for t in &tables_with_offsets {
        out.extend_from_slice(&t.tag);
        out.extend_from_slice(&t.woff_offset.to_be_bytes());
        out.extend_from_slice(&t.comp_length.to_be_bytes());
        out.extend_from_slice(&t.orig_length.to_be_bytes());
        out.extend_from_slice(&t.orig_checksum.to_be_bytes());
    }
    debug_assert_eq!(out.len(), data_start);

    // ---- Write padded table data --------------------------------------------
    for t in &tables_with_offsets {
        out.extend_from_slice(&t.data);
        // Pad to 4-byte boundary.
        let rem = t.data.len() % 4;
        if rem != 0 {
            let pad_len = 4 - rem;
            out.resize(out.len() + pad_len, 0u8);
        }
    }

    Ok(out)
}

// ----------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sfnt_rejects_too_short() {
        let result = parse_sfnt(&[0u8; 4]);
        assert!(matches!(result, Err(WebFontError::TooShort)));
    }

    #[test]
    fn encode_handles_empty_table_list_sfnt() {
        // Build a minimal SFNT with zero tables.
        let sfnt = crate::sfnt::build_sfnt(crate::sfnt::SFNT_MAGIC_TT, &[]).expect("empty SFNT");
        // Zero-table SFNT should encode without error.
        let woff = encode(&sfnt).expect("encode should succeed");
        assert!(woff.len() >= WOFF1_HEADER_SIZE);
    }
}
