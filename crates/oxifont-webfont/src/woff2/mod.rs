//! WOFF2 decoder.
//!
//! Parses the WOFF2 header and table directory, decompresses the single
//! brotli-compressed font data block, applies any table transforms (glyf/loca,
//! hmtx), and reassembles a valid SFNT byte buffer.
//!
//! Reference: W3C WOFF2 specification
//! <https://www.w3.org/TR/WOFF2/>

/// WOFF2 encoder.
pub mod encode;
/// WOFF2 transformed glyf/loca reconstruction.
pub mod glyf;
/// WOFF2 header and table directory parsing.
pub mod header;
/// WOFF2 transformed hmtx reconstruction.
pub mod hmtx;
/// Streaming WOFF2 decoder.
pub mod streaming;

pub use streaming::decode_streaming;

use oxiarc_brotli::decompress;

use crate::error::WebFontError;
use crate::sfnt::{build_sfnt, detect_sfnt_version};

use header::{parse_header, parse_table_directory, read_255_u16_slice, Woff2TableEntry};
use hmtx::{extract_glyf_xmins, reconstruct_hmtx};

/// (glyf table bytes, loca table bytes, loca offsets for hmtx xMin lookup).
type GlyfLocaResult = (Option<Vec<u8>>, Option<Vec<u8>>, Option<Vec<u32>>);

/// List of (4-byte tag, table data) pairs forming a decoded SFNT.
type TableList = Vec<([u8; 4], Vec<u8>)>;

/// WOFF2 `ttcf` flavor: indicates a TrueType Collection (font collection).
const WOFF2_COLLECTION_FLAVOR: u32 = 0x7474_6366; // b"ttcf"

// WOFF2 header offsets for metadata fields.
const WOFF2_META_OFFSET_POS: usize = 28;
const WOFF2_META_LENGTH_POS: usize = 32;

// WOFF2 header offsets for private data block fields.
// Layout (per WOFF2 spec §4, immediately after metaOrigLength @ 36):
//   privOffset @ 40  (u32, offset from start of WOFF2 file)
//   privLength @ 44  (u32, byte count of the private data block)
const WOFF2_PRIV_OFFSET_POS: usize = 40;
const WOFF2_PRIV_LENGTH_POS: usize = 44;

// ----------------------------------------------------------------- public API

/// Decode a WOFF2 file into an SFNT byte buffer together with optional
/// extended metadata XML (if the WOFF2 file contains a metadata block).
///
/// The metadata block, when present, is brotli-decompressed and returned as a
/// UTF-8 string. Lossy conversion is used for byte sequences that are not
/// valid UTF-8.
///
/// This is the low-level variant used by [`crate::detect::decode_auto`].
pub fn decode_with_metadata(data: &[u8]) -> Result<(Vec<u8>, Option<String>), WebFontError> {
    let hdr = parse_header(data)?;

    // Read metadata offset/length from the fixed header positions before we
    // start consuming `data` for table directory parsing.
    let meta_offset = header::read_u32(data, WOFF2_META_OFFSET_POS)?;
    let meta_length = header::read_u32(data, WOFF2_META_LENGTH_POS)?;

    let (dir, dir_end) = parse_table_directory(data, hdr.num_tables)?;

    // The compressed font data block starts at dir_end.
    let compressed_size = hdr.total_compressed_size as usize;
    let compressed_end = dir_end
        .checked_add(compressed_size)
        .ok_or(WebFontError::Overflow("compressed block end"))?;

    let compressed_data = data
        .get(dir_end..compressed_end)
        .ok_or(WebFontError::OutOfBounds {
            context: "compressed font data block",
        })?;

    // Decompress the single brotli stream.
    // NOTE: the WOFF2 header's totalSfntSize provides an upper bound on the
    // expected decompressed output size.  Pre-allocating the decompressed
    // buffer with that capacity hint would reduce Vec reallocations; however,
    // the current oxiarc-brotli `decompress` API returns its own owned
    // `Vec<u8>` and does not accept a caller-supplied capacity.  Pre-allocation
    // is therefore deferred until oxiarc-brotli exposes a
    // `decompress_with_capacity(data, hint_bytes)` form.
    let font_data =
        decompress(compressed_data).map_err(|e| WebFontError::DecompressError(e.to_string()))?;

    // Distribute decompressed bytes to tables.
    // We need to extract glyf/loca transform data and hmtx transform data separately.
    let tables = extract_and_transform_tables(&dir, &font_data)?;

    let sfnt_version = if hdr.sf_version == 0x0001_0000 || hdr.sf_version == 0x4F54_544F {
        hdr.sf_version
    } else {
        detect_sfnt_version(&tables)
    };

    let sfnt = build_sfnt(sfnt_version, &tables)?;

    // Extract metadata block (brotli-compressed XML, WOFF2 spec §6).
    let metadata = extract_woff2_metadata(data, meta_offset, meta_length)?;

    Ok((sfnt, metadata))
}

/// Extract and decompress the WOFF2 metadata block, if present.
fn extract_woff2_metadata(
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
        .ok_or(WebFontError::Overflow("WOFF2 metadata block end"))?;
    let compressed = data.get(start..end).ok_or(WebFontError::OutOfBounds {
        context: "WOFF2 metadata block",
    })?;

    let decompressed = decompress(compressed)
        .map_err(|e| WebFontError::DecompressError(format!("WOFF2 metadata: {e}")))?;

    Ok(Some(String::from_utf8_lossy(&decompressed).into_owned()))
}

/// Decode a WOFF2 file into an SFNT byte buffer.
pub fn decode(data: &[u8]) -> Result<Vec<u8>, WebFontError> {
    decode_with_metadata(data).map(|(sfnt, _)| sfnt)
}

/// Extract the private data block from a WOFF2 file, if present.
///
/// The private data block is an opaque byte range embedded in the WOFF2 file
/// between the compressed font data block and the optional metadata block.
/// Its presence and location are indicated by the `privOffset` and `privLength`
/// fields in the WOFF2 file header (spec §4).
///
/// Returns `None` if:
/// - `data` is shorter than the 48-byte WOFF2 header,
/// - the WOFF2 signature is wrong,
/// - `privOffset` or `privLength` is zero, or
/// - the claimed byte range falls outside `data`.
pub fn extract_woff2_private_data(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < header::WOFF2_HEADER_SIZE {
        return None;
    }

    // Verify WOFF2 signature so we don't misinterpret other formats.
    let signature = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
    if signature != header::WOFF2_SIGNATURE {
        return None;
    }

    let priv_offset = header::read_u32(data, WOFF2_PRIV_OFFSET_POS).ok()? as usize;
    let priv_length = header::read_u32(data, WOFF2_PRIV_LENGTH_POS).ok()? as usize;

    if priv_offset == 0 || priv_length == 0 {
        return None;
    }

    let end = priv_offset.checked_add(priv_length)?;
    data.get(priv_offset..end).map(|s| s.to_vec())
}

// ---------------------------------------------------------- collection decoding

/// Parsed per-font entry from the WOFF2 `CollectionHeader`.
struct CollectionFontEntry {
    /// The SFNT flavor (0x00010000 for TrueType, `OTTO` for CFF).
    flavor: u32,
    /// Indices into the shared WOFF2 table directory.
    table_indices: Vec<u16>,
}

/// Decode a WOFF2 font collection (flavor == `ttcf`) into a `Vec` of SFNT byte
/// buffers, one per font in the collection.
///
/// This implementation follows the typical TTC-in-WOFF2 layout where glyf and
/// loca are shared across all fonts (single transformed entry in the shared
/// directory).  Collections with multiple independently-transformed glyf/loca
/// entries are not fully supported; only the first transformed glyf/loca block
/// is reconstructed and used for all glyf/loca slots.
///
/// # Errors
/// Returns [`WebFontError`] if:
/// - the data is not a valid WOFF2 file,
/// - the flavor is not `ttcf` (`0x74746366`),
/// - the decompressed stream is malformed, or
/// - any table index is out of bounds.
pub fn decode_collection(data: &[u8]) -> Result<Vec<Vec<u8>>, WebFontError> {
    let hdr = parse_header(data)?;

    if hdr.sf_version != WOFF2_COLLECTION_FLAVOR {
        return Err(WebFontError::InvalidField {
            field: "flavor (expected ttcf for collection)",
            value: hdr.sf_version as u64,
        });
    }

    let (dir, dir_end) = parse_table_directory(data, hdr.num_tables)?;

    // Locate and decompress the single brotli stream.
    let compressed_size = hdr.total_compressed_size as usize;
    let compressed_end = dir_end
        .checked_add(compressed_size)
        .ok_or(WebFontError::Overflow("compressed block end"))?;
    let compressed_data = data
        .get(dir_end..compressed_end)
        .ok_or(WebFontError::OutOfBounds {
            context: "compressed font data block",
        })?;

    let font_data =
        decompress(compressed_data).map_err(|e| WebFontError::DecompressError(e.to_string()))?;

    // The decompressed stream begins with the CollectionHeader, followed by the
    // per-font table sub-streams.  Parse that header first.
    let (font_entries, table_data_offset) = parse_collection_header(&font_data)?;

    // Slice the remaining decompressed data (the actual table payload) and build
    // the shared, transformed table list.  Note: the table data immediately
    // follows the CollectionHeader in the decompressed stream.
    let table_payload = font_data
        .get(table_data_offset..)
        .ok_or(WebFontError::OutOfBounds {
            context: "collection table payload",
        })?;

    // Build the index-keyed table array: indexed_tables[i] is the reconstructed
    // data for dir[i], preserving dir position as the key.  This correctly handles
    // collections whose shared directory contains multiple entries with the same tag
    // (e.g., per-face `cmap`, `name`, or `hmtx` tables).
    let indexed_tables = extract_tables_by_index(&dir, table_payload)?;

    // For each CollectionFontEntry, pick the referenced tables and assemble an SFNT.
    let mut results: Vec<Vec<u8>> = Vec::with_capacity(font_entries.len());

    for entry in &font_entries {
        let font_tables = select_font_tables_indexed(&entry.table_indices, &dir, &indexed_tables)?;
        let sfnt = build_sfnt(entry.flavor, &font_tables)?;
        results.push(sfnt);
    }

    Ok(results)
}

/// Parse the `CollectionHeader` from the decompressed data stream.
///
/// Returns the list of per-font entries and the byte offset at which the
/// shared table data begins (i.e., immediately after the `CollectionHeader`).
fn parse_collection_header(data: &[u8]) -> Result<(Vec<CollectionFontEntry>, usize), WebFontError> {
    // CollectionHeader layout:
    //   version   u32
    //   numFonts  255UInt16
    let mut pos = 0usize;

    // version (4 bytes)
    if data.len() < 4 {
        return Err(WebFontError::TooShort);
    }
    let version = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
    pos += 4;

    // CollectionHeader version must be 0x00010000 or 0x00020000.
    if version != 0x0001_0000 && version != 0x0002_0000 {
        return Err(WebFontError::InvalidField {
            field: "CollectionHeader.version",
            value: version as u64,
        });
    }

    // numFonts (255UInt16)
    let (num_fonts, consumed) = read_255_u16_slice(data, pos)?;
    pos += consumed;

    let num_fonts = num_fonts as usize;
    let mut entries = Vec::with_capacity(num_fonts);

    for _ in 0..num_fonts {
        // numTables (255UInt16)
        let (num_tables, consumed) = read_255_u16_slice(data, pos)?;
        pos += consumed;

        // flavor (u32)
        let flavor = data
            .get(pos..pos + 4)
            .ok_or(WebFontError::TooShort)
            .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))?;
        pos += 4;

        // tableIndices[numTables] (255UInt16 each)
        let mut table_indices = Vec::with_capacity(num_tables as usize);
        for _ in 0..num_tables {
            let (idx, consumed) = read_255_u16_slice(data, pos)?;
            pos += consumed;
            table_indices.push(idx);
        }

        entries.push(CollectionFontEntry {
            flavor,
            table_indices,
        });
    }

    Ok((entries, pos))
}

/// Build a directory-index-keyed table array from the decompressed font data.
///
/// Returns a `Vec<Vec<u8>>` where `result[i]` contains the (possibly
/// transform-reconstructed) data for `dir[i]`.  This preserves directory
/// position as the key, which is required for collections whose shared
/// directory contains multiple entries with the same tag (e.g., per-face
/// `cmap`, `name`, or `hmtx` tables).
fn extract_tables_by_index(
    dir: &[Woff2TableEntry],
    font_data: &[u8],
) -> Result<Vec<Vec<u8>>, WebFontError> {
    // Step 1: assign raw byte slices from the decompressed stream to each dir entry.
    let mut raw_slices: Vec<Vec<u8>> = Vec::with_capacity(dir.len());
    let mut offset = 0usize;

    for entry in dir {
        let len = entry.transform_length as usize;
        let slice = font_data
            .get(offset..offset + len)
            .ok_or(WebFontError::OutOfBounds {
                context: "table slice in decompressed collection data",
            })?;
        raw_slices.push(slice.to_vec());
        offset = offset.checked_add(len).ok_or(WebFontError::Overflow(
            "table offset in decompressed collection data",
        ))?;
    }

    // Step 2: detect and apply glyf/loca transform (if present in the directory).
    // In collections, glyf+loca are usually shared (single entry each).
    let glyf_idx = dir.iter().position(|e| &e.tag == b"glyf");
    // loca is reconstructed as part of the glyf transform; no separate lookup needed.

    let has_glyf_transform = glyf_idx.is_some_and(|i| dir[i].is_transformed());

    // Reconstruct glyf+loca from the transformed sub-stream if needed.
    let (glyf_reconstructed, loca_reconstructed, loca_offsets): GlyfLocaResult =
        if has_glyf_transform {
            let glyf_raw =
                glyf_idx
                    .and_then(|i| raw_slices.get(i))
                    .ok_or(WebFontError::Unsupported(
                        "glyf transform without glyf data",
                    ))?;
            let reconstructed = glyf::reconstruct_glyf_loca(glyf_raw)?;
            let offsets = loca_offsets_from_loca(&reconstructed.loca, reconstructed.index_format)?;
            Ok::<GlyfLocaResult, WebFontError>((
                Some(reconstructed.glyf),
                Some(reconstructed.loca),
                Some(offsets),
            ))
        } else {
            Ok::<GlyfLocaResult, WebFontError>((None, None, None))
        }?;

    // Step 3: detect and apply hmtx transform (if present).
    let hmtx_idx = dir.iter().position(|e| &e.tag == b"hmtx");
    let has_hmtx_transform = hmtx_idx.is_some_and(|i| dir[i].is_transformed());

    let (num_h_metrics, num_glyphs): (u16, u16) = if has_hmtx_transform {
        let hhea_idx = dir.iter().position(|e| &e.tag == b"hhea");
        let maxp_idx = dir.iter().position(|e| &e.tag == b"maxp");

        let nhm = hhea_idx
            .and_then(|i| raw_slices.get(i))
            .and_then(|d| d.get(34..36))
            .map(|b| u16::from_be_bytes([b[0], b[1]]))
            .unwrap_or(0);
        let ng = maxp_idx
            .and_then(|i| raw_slices.get(i))
            .and_then(|d| d.get(4..6))
            .map(|b| u16::from_be_bytes([b[0], b[1]]))
            .unwrap_or(0);
        (nhm, ng)
    } else {
        (0, 0)
    };

    // Step 4: assemble the final per-index result, applying reconstructed versions
    // where transforms were applied.
    let mut result: Vec<Vec<u8>> = Vec::with_capacity(dir.len());

    for (i, entry) in dir.iter().enumerate() {
        let tag = entry.tag;
        if has_glyf_transform && &tag == b"glyf" {
            result.push(glyf_reconstructed.clone().ok_or(WebFontError::Unsupported(
                "glyf reconstruction produced None",
            ))?);
        } else if has_glyf_transform && &tag == b"loca" {
            result.push(loca_reconstructed.clone().ok_or(WebFontError::Unsupported(
                "loca reconstruction produced None",
            ))?);
        } else if has_hmtx_transform && &tag == b"hmtx" {
            // Reconstruct hmtx using whichever glyf data is available.
            let glyf_data_for_hmtx = if has_glyf_transform {
                glyf_reconstructed.as_deref()
            } else {
                glyf_idx
                    .and_then(|gi| raw_slices.get(gi))
                    .map(|v| v.as_slice())
            };
            let glyf_xmins: Vec<i16> = match (&loca_offsets, glyf_data_for_hmtx) {
                (Some(offsets), Some(glyf_bytes)) => {
                    extract_glyf_xmins(glyf_bytes, offsets, num_glyphs)
                }
                _ => vec![0i16; num_glyphs as usize],
            };
            let hmtx_raw = raw_slices.get(i).ok_or(WebFontError::OutOfBounds {
                context: "hmtx raw slice",
            })?;
            let hmtx = reconstruct_hmtx(hmtx_raw, num_glyphs, num_h_metrics, &glyf_xmins)?;
            result.push(hmtx);
        } else {
            // No transform: use raw slice as-is.
            result.push(
                raw_slices
                    .get(i)
                    .ok_or(WebFontError::OutOfBounds {
                        context: "raw table slice",
                    })?
                    .clone(),
            );
        }
    }

    Ok(result)
}

/// Select and assemble the table list for one font from the collection,
/// using directory-index-keyed data to correctly handle duplicate tags.
///
/// `table_indices` are indices into the shared WOFF2 table directory `dir`.
/// `indexed_tables[i]` is the reconstructed data for `dir[i]`.
fn select_font_tables_indexed(
    table_indices: &[u16],
    dir: &[Woff2TableEntry],
    indexed_tables: &[Vec<u8>],
) -> Result<TableList, WebFontError> {
    let mut selected: TableList = Vec::with_capacity(table_indices.len());

    for &idx in table_indices {
        let i = idx as usize;
        let entry = dir.get(i).ok_or(WebFontError::OutOfBounds {
            context: "CollectionFontEntry table index out of bounds",
        })?;
        let table_data = indexed_tables.get(i).ok_or(WebFontError::OutOfBounds {
            context: "CollectionFontEntry table index exceeds indexed_tables length",
        })?;

        selected.push((entry.tag, table_data.clone()));
    }

    // Sort by tag for SFNT interoperability.
    selected.sort_by_key(|(tag, _)| *tag);

    Ok(selected)
}

// --------------------------------------------------------------------- helpers

/// Slice the decompressed font data into per-table buffers and apply transforms.
pub(crate) fn extract_and_transform_tables(
    dir: &[Woff2TableEntry],
    font_data: &[u8],
) -> Result<TableList, WebFontError> {
    // First pass: assign raw slices from font_data to each table.
    let mut raw_slices: Vec<(&[u8], &Woff2TableEntry)> = Vec::with_capacity(dir.len());
    let mut offset = 0usize;

    for entry in dir {
        let len = entry.transform_length as usize;
        let slice = font_data
            .get(offset..offset + len)
            .ok_or(WebFontError::OutOfBounds {
                context: "table slice in decompressed data",
            })?;
        raw_slices.push((slice, entry));
        offset = offset
            .checked_add(len)
            .ok_or(WebFontError::Overflow("table offset in decompressed data"))?;
    }

    // Locate glyf and loca entries (needed together for the transform).
    let glyf_raw = raw_slices.iter().find(|(_, e)| &e.tag == b"glyf");
    let loca_raw = raw_slices.iter().find(|(_, e)| &e.tag == b"loca");

    let has_glyf_transform = glyf_raw.is_some_and(|(_, e)| e.is_transformed());
    let has_loca_transform = loca_raw.is_some_and(|(_, e)| e.is_transformed());

    // Reconstruct glyf+loca if transformed.
    let (glyf_table, loca_table, glyf_loca_offsets): GlyfLocaResult = if has_glyf_transform {
        let (glyf_data, _) =
            glyf_raw.ok_or(WebFontError::Unsupported("glyf transform without glyf"))?;
        let reconstructed = glyf::reconstruct_glyf_loca(glyf_data)?;

        // Build loca offset array for hmtx reconstruction.
        let offsets = loca_offsets_from_loca(&reconstructed.loca, reconstructed.index_format)?;

        Ok::<GlyfLocaResult, WebFontError>((
            Some(reconstructed.glyf),
            Some(reconstructed.loca),
            Some(offsets),
        ))
    } else {
        let glyf_bytes = glyf_raw.map(|(d, _)| d.to_vec());
        let loca_bytes = loca_raw.map(|(d, _)| d.to_vec());
        Ok::<GlyfLocaResult, WebFontError>((glyf_bytes, loca_bytes, None))
    }?;

    // Locate hmtx transform if any.
    let hmtx_entry = raw_slices.iter().find(|(_, e)| &e.tag == b"hmtx");
    let has_hmtx_transform = hmtx_entry.is_some_and(|(_, e)| e.is_transformed());

    // We need hhea.numberOfHMetrics and maxp.numGlyphs for hmtx reconstruction.
    let (num_h_metrics, num_glyphs): (u16, u16) = if has_hmtx_transform {
        let hhea_bytes = raw_slices
            .iter()
            .find(|(_, e)| &e.tag == b"hhea")
            .map(|(d, _)| *d);
        let maxp_bytes = raw_slices
            .iter()
            .find(|(_, e)| &e.tag == b"maxp")
            .map(|(d, _)| *d);

        let nhm = hhea_bytes
            .and_then(|d| d.get(34..36))
            .map(|b| u16::from_be_bytes([b[0], b[1]]))
            .unwrap_or(0);
        let ng = maxp_bytes
            .and_then(|d| d.get(4..6))
            .map(|b| u16::from_be_bytes([b[0], b[1]]))
            .unwrap_or(0);

        (nhm, ng)
    } else {
        (0, 0)
    };

    // Build final table list.
    let mut tables: Vec<([u8; 4], Vec<u8>)> = Vec::with_capacity(dir.len());

    for (raw, entry) in &raw_slices {
        let tag = entry.tag;

        // Skip glyf+loca when we have a transformed version (handled separately).
        if has_glyf_transform && (&tag == b"glyf" || &tag == b"loca") {
            continue;
        }
        if has_loca_transform && &tag == b"loca" && !has_glyf_transform {
            // loca-only transform (unusual): skip, already handled.
            continue;
        }
        if has_hmtx_transform && &tag == b"hmtx" {
            continue;
        }

        tables.push((tag, raw.to_vec()));
    }

    // Insert reconstructed glyf+loca.
    if has_glyf_transform {
        if let Some(glyf) = glyf_table {
            tables.push((*b"glyf", glyf));
        }
        if let Some(loca) = loca_table {
            tables.push((*b"loca", loca));
        }
    }

    // Insert reconstructed hmtx.
    if has_hmtx_transform {
        if let Some((hmtx_data, _)) = hmtx_entry {
            let glyf_entry = tables.iter().find(|(t, _)| t == b"glyf").map(|(_, d)| d);
            let glyf_xmins: Vec<i16> = match (&glyf_loca_offsets, glyf_entry) {
                (Some(offsets), Some(glyf_bytes)) => {
                    extract_glyf_xmins(glyf_bytes, offsets, num_glyphs)
                }
                _ => vec![0i16; num_glyphs as usize],
            };

            let hmtx = reconstruct_hmtx(hmtx_data, num_glyphs, num_h_metrics, &glyf_xmins)?;
            tables.push((*b"hmtx", hmtx));
        }
    }

    // Sort by tag for maximum interoperability.
    tables.sort_by_key(|(tag, _)| *tag);

    Ok(tables)
}

/// Parse a loca table into a Vec<u32> of byte offsets.
fn loca_offsets_from_loca(loca: &[u8], index_format: u16) -> Result<Vec<u32>, WebFontError> {
    if index_format == 0 {
        // Short loca: uint16 pairs, value × 2 = offset.
        if !loca.len().is_multiple_of(2) {
            return Err(WebFontError::MalformedGlyfTransform(
                "short loca odd length".to_string(),
            ));
        }
        let mut offsets = Vec::with_capacity(loca.len() / 2);
        for chunk in loca.chunks_exact(2) {
            let short = u16::from_be_bytes([chunk[0], chunk[1]]);
            offsets.push((short as u32) * 2);
        }
        Ok(offsets)
    } else {
        // Long loca: uint32 direct offsets.
        if !loca.len().is_multiple_of(4) {
            return Err(WebFontError::MalformedGlyfTransform(
                "long loca not multiple of 4".to_string(),
            ));
        }
        let mut offsets = Vec::with_capacity(loca.len() / 4);
        for chunk in loca.chunks_exact(4) {
            let off = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            offsets.push(off);
        }
        Ok(offsets)
    }
}

// ----------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_rejects_bad_signature() {
        let bad = vec![0u8; 60];
        let result = decode(&bad);
        assert!(matches!(result, Err(WebFontError::InvalidSignature)));
    }

    #[test]
    fn decode_rejects_too_short() {
        let short = vec![0u8; 10];
        let result = decode(&short);
        assert!(matches!(result, Err(WebFontError::TooShort)));
    }

    #[test]
    fn loca_offsets_short_format() {
        // [0x00, 0x00, 0x00, 0x32] → [0, 100].
        let loca = [0x00u8, 0x00, 0x00, 0x32];
        let offsets = loca_offsets_from_loca(&loca, 0).expect("should parse");
        assert_eq!(offsets, &[0, 100]);
    }

    #[test]
    fn loca_offsets_long_format() {
        let loca = [0x00u8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x64];
        let offsets = loca_offsets_from_loca(&loca, 1).expect("should parse");
        assert_eq!(offsets, &[0, 100]);
    }
}
