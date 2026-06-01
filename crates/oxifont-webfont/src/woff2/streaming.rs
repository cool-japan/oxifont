//! Streaming WOFF2 decoder — decodes from any `impl Read` without buffering the compressed payload.
//!
//! Unlike the one-shot decoder which requires the entire WOFF2 file as a `&[u8]` slice,
//! this module reads the WOFF2 header and table directory via fixed-size and byte-at-a-time
//! reads, then uses `oxiarc_brotli::streaming::BrotliDecompressor` to stream the
//! brotli-compressed font data block from the reader. The decompressed bytes are collected
//! into a `Vec<u8>` and sliced per-table using the existing transform and assembly helpers.
//!
//! # Note on memory behaviour
//!
//! The brotli decompressor (`oxiarc_brotli::streaming::BrotliDecompressor`) currently
//! reads all compressed bytes into an internal buffer before decompressing, so the
//! compressed payload is still allocated on the heap inside the brotli layer. The benefit
//! of this API is that the *caller* never needs to hold the compressed bytes — the reader
//! can be a network socket, a file, or any other `impl Read` source. Future improvements
//! to oxiarc-brotli may introduce true incremental decompression, making this path
//! fully allocation-free for the compressed payload.

use std::io::{BufReader, Read};

use oxiarc_brotli::streaming::BrotliDecompressor;

use crate::error::WebFontError;
use crate::sfnt::{build_sfnt, detect_sfnt_version};
use crate::woff2::header::{
    decode_uint_base128, needs_transform_length, Woff2Header, Woff2TableEntry, KNOWN_TAGS,
    WOFF2_HEADER_SIZE, WOFF2_SIGNATURE,
};

// Use the crate-internal helper from the parent module (woff2/mod.rs).
use super::extract_and_transform_tables;

// ------------------------------------------------------------------ constants

/// Byte offset of the SFNT flavor field in the WOFF2 header.
const HDR_FLAVOR_OFFSET: usize = 4;
/// Byte offset of the numTables field in the WOFF2 header.
const HDR_NUM_TABLES_OFFSET: usize = 12;
/// Byte offset of the reserved field in the WOFF2 header.
const HDR_RESERVED_OFFSET: usize = 14;
/// Byte offset of the totalSfntSize field in the WOFF2 header.
const HDR_TOTAL_SFNT_SIZE_OFFSET: usize = 16;
/// Byte offset of the totalCompressedSize field in the WOFF2 header.
const HDR_TOTAL_COMPRESSED_SIZE_OFFSET: usize = 20;

// ------------------------------------------------------------------ helpers

/// Read a big-endian u16 from a fixed-size header buffer.
#[inline]
fn hdr_u16(buf: &[u8; WOFF2_HEADER_SIZE], offset: usize) -> u16 {
    u16::from_be_bytes([buf[offset], buf[offset + 1]])
}

/// Read a big-endian u32 from a fixed-size header buffer.
#[inline]
fn hdr_u32(buf: &[u8; WOFF2_HEADER_SIZE], offset: usize) -> u32 {
    u32::from_be_bytes([
        buf[offset],
        buf[offset + 1],
        buf[offset + 2],
        buf[offset + 3],
    ])
}

// ----------------------------------------- streaming header parse

/// Parse the fixed 48-byte WOFF2 header from a buffer, validating magic and reserved.
fn parse_header_from_buf(buf: &[u8; WOFF2_HEADER_SIZE]) -> Result<Woff2Header, WebFontError> {
    let signature = hdr_u32(buf, 0);
    if signature != WOFF2_SIGNATURE {
        return Err(WebFontError::InvalidSignature);
    }

    let sf_version = hdr_u32(buf, HDR_FLAVOR_OFFSET);
    let num_tables = hdr_u16(buf, HDR_NUM_TABLES_OFFSET);
    let reserved = hdr_u16(buf, HDR_RESERVED_OFFSET);
    if reserved != 0 {
        return Err(WebFontError::InvalidField {
            field: "reserved",
            value: reserved as u64,
        });
    }
    let total_sfnt_size = hdr_u32(buf, HDR_TOTAL_SFNT_SIZE_OFFSET);
    let total_compressed_size = hdr_u32(buf, HDR_TOTAL_COMPRESSED_SIZE_OFFSET);

    Ok(Woff2Header {
        sf_version,
        num_tables,
        total_compressed_size,
        total_sfnt_size,
    })
}

// -------------------------------------- streaming table directory parse

/// Read the WOFF2 table directory from a `BufReader`.
///
/// Each entry uses variable-length encoding (UIntBase128 for lengths) and must
/// be read byte-by-byte. A small staging buffer is used to drive the existing
/// `decode_uint_base128` slice decoder.
fn parse_table_directory_from_reader<R: Read>(
    reader: &mut BufReader<R>,
    num_tables: u16,
) -> Result<Vec<Woff2TableEntry>, WebFontError> {
    let mut entries = Vec::with_capacity(num_tables as usize);

    for _ in 0..num_tables {
        // Read the flags byte.
        let flags_byte = read_one_byte(reader)?;

        let tag_idx = flags_byte & 0x3F;
        let transform_version = (flags_byte >> 6) & 0x03;

        let tag: [u8; 4] = if tag_idx == 63 {
            // Arbitrary 4-byte tag follows.
            let mut tag_buf = [0u8; 4];
            reader.read_exact(&mut tag_buf)?;
            tag_buf
        } else {
            let known = KNOWN_TAGS
                .get(tag_idx as usize)
                .ok_or(WebFontError::InvalidField {
                    field: "tag_index",
                    value: tag_idx as u64,
                })?;
            **known
        };

        // origLength: UIntBase128 — read up to 5 bytes until no-continuation bit.
        let orig_length = read_uint_base128_from_reader(reader)?;

        // transformLength: present only for glyf/loca with transform_version==0
        // and hmtx with transform_version==1 (per WOFF2 spec §5, Table 3).
        let transform_length = if needs_transform_length(tag, transform_version) {
            read_uint_base128_from_reader(reader)?
        } else {
            orig_length
        };

        entries.push(Woff2TableEntry {
            tag,
            flags: flags_byte,
            transform_version,
            orig_length,
            transform_length,
        });
    }

    Ok(entries)
}

/// Read a single byte from a `BufReader`.
fn read_one_byte<R: Read>(reader: &mut BufReader<R>) -> Result<u8, WebFontError> {
    let mut buf = [0u8; 1];
    reader.read_exact(&mut buf)?;
    Ok(buf[0])
}

/// Decode a WOFF2 `UIntBase128` variable-length integer from a `BufReader`.
///
/// Reads bytes one at a time until the continuation bit (MSB) is clear.
/// Reuses the `decode_uint_base128` slice decoder to avoid duplicating logic.
fn read_uint_base128_from_reader<R: Read>(reader: &mut BufReader<R>) -> Result<u32, WebFontError> {
    // UIntBase128 is at most 5 bytes. Stage bytes until terminal.
    let mut staging = [0u8; 5];
    let mut len = 0usize;

    loop {
        if len >= 5 {
            return Err(WebFontError::Overflow("UIntBase128 exceeds 5 bytes"));
        }
        let b = read_one_byte(reader)?;
        staging[len] = b;
        len += 1;
        if b & 0x80 == 0 {
            // No continuation bit — complete value.
            break;
        }
    }

    let (value, _consumed) = decode_uint_base128(&staging[..len])?;
    Ok(value)
}

// ---------------------------------------------------------------- public API

/// Decode a WOFF2 stream into an SFNT byte buffer.
///
/// Reads the WOFF2 header and table directory from `reader`, then streams
/// the brotli-compressed font data block through
/// [`BrotliDecompressor`] bounded by `totalCompressedSize` using
/// [`Read::take`].  The decompressed bytes are passed to the existing
/// table-transform and SFNT-assembly helpers.
///
/// # Errors
///
/// Returns [`WebFontError`] on:
/// - I/O errors reading from `reader`,
/// - invalid WOFF2 magic or reserved fields,
/// - varint decode failure,
/// - brotli decompression failure, or
/// - malformed table transforms.
pub fn decode_streaming<R: Read>(reader: R) -> Result<Vec<u8>, WebFontError> {
    let mut buf_reader = BufReader::new(reader);

    // Step 1: read and parse the fixed 48-byte WOFF2 header.
    let mut header_buf = [0u8; WOFF2_HEADER_SIZE];
    buf_reader.read_exact(&mut header_buf)?;
    let hdr = parse_header_from_buf(&header_buf)?;

    // Step 2: read the variable-length table directory.
    let dir = parse_table_directory_from_reader(&mut buf_reader, hdr.num_tables)?;

    // Step 3: stream-decompress the brotli payload.
    //
    // `BufReader::take(n)` limits the reader to exactly `n` bytes of the
    // compressed block, preventing the decompressor from consuming any bytes
    // that belong to the metadata or private-data block that follow.
    let compressed_size = hdr.total_compressed_size as u64;
    let mut limited = buf_reader.take(compressed_size);
    let mut decomp_reader = BrotliDecompressor::new(&mut limited);

    let mut font_data = Vec::new();
    decomp_reader
        .read_to_end(&mut font_data)
        .map_err(|e| WebFontError::DecompressError(e.to_string()))?;

    // Step 4: distribute decompressed bytes to tables and apply transforms
    // (glyf/loca reconstruction via woff2::glyf, hmtx reconstruction via woff2::hmtx).
    let tables = extract_and_transform_tables(&dir, &font_data)?;

    // Step 5: determine SFNT version and assemble the output SFNT buffer.
    let sfnt_version = if hdr.sf_version == 0x0001_0000 || hdr.sf_version == 0x4F54_544F {
        hdr.sf_version
    } else {
        detect_sfnt_version(&tables)
    };

    build_sfnt(sfnt_version, &tables)
}
