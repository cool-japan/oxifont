//! Partial SFNT reader for lazy metadata extraction.
//!
//! Reads only the `name`, `OS/2`, and `cmap` tables from a font file via
//! `File::seek` + `read_exact`, then reconstructs a minimal valid SFNT in
//! memory and delegates to [`oxifont_parser::ParsedFace::parse`] to populate
//! all [`FaceInfo`] fields — no custom per-table parsing needed.
//!
//! This is substantially faster than reading the full font file when the font
//! contains large `glyf`/`loca`/`hmtx`/`gvar` tables (typically 90–99% of a
//! font file's bytes).

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Arc;

use oxifont_core::{FaceInfo, FontError};
use oxifont_parser::{face_count, ParsedFace};

/// Tags of the tables needed for metadata extraction.
///
/// - `head`, `hhea`, `maxp`: required by `ttf_parser` for any parse to succeed.
/// - `name`: family name, PostScript name.
/// - `OS/2`: style, weight, stretch.
/// - `cmap`: unicode coverage (character map).
const REQUIRED_TAGS: [&[u8; 4]; 6] = [b"head", b"hhea", b"maxp", b"name", b"OS/2", b"cmap"];

/// Read [`FaceInfo`] metadata for every face in a single font file without
/// loading the full font data.
///
/// Only the SFNT table directory plus the `name`, `OS/2`, and `cmap` tables
/// are read from disk. A minimal SFNT is reconstructed in memory and fed to
/// `ParsedFace::parse`, which extracts all `FaceInfo` fields via the standard
/// parser. Unknown or missing tables are silently skipped — the reconstructed
/// SFNT will simply lack those tables and the parser will use its normal
/// fallback paths.
///
/// # Errors
///
/// Returns [`FontError::IoError`] when the file cannot be opened or read, and
/// [`FontError::ParseError`] when the reconstructed SFNT cannot be parsed.
pub fn read_face_metadata_partial(path: &Path) -> Result<Vec<FaceInfo>, FontError> {
    let mut file = std::fs::File::open(path)?;

    // -------------------------------------------------------------------------
    // 1. Read the 12-byte SFNT offset table.
    //    Layout: sfVersion(u32) numTables(u16) searchRange(u16)
    //            entrySelector(u16) rangeShift(u16)
    // -------------------------------------------------------------------------
    let mut header = [0u8; 12];
    file.read_exact(&mut header)
        .map_err(|e| FontError::IoError(Arc::new(e)))?;

    // Detect TTC: fall back to full parse for collections because each
    // sub-face's table directory has a different offset and seeking each one
    // adds complexity. TTC files are not the common case for lazy scanning.
    let magic = &header[..4];
    if magic == b"ttcf" {
        // Read the full file and let ParsedFace handle it properly.
        return read_all_faces_full(path);
    }

    let num_tables = u16::from_be_bytes([header[4], header[5]]) as usize;

    // -------------------------------------------------------------------------
    // 2. Read the table directory: num_tables × 16 bytes.
    //    Each entry: tag(4) checksum(4) offset(4) length(4)
    // -------------------------------------------------------------------------
    let dir_size = num_tables * 16;
    let mut dir_buf = vec![0u8; dir_size];
    file.read_exact(&mut dir_buf)
        .map_err(|e| FontError::IoError(Arc::new(e)))?;

    // Parse directory entries into (tag, offset, length).
    let entries: Vec<([u8; 4], u32, u32)> = (0..num_tables)
        .filter_map(|i| {
            let base = i * 16;
            let tag: [u8; 4] = dir_buf[base..base + 4].try_into().ok()?;
            let offset = u32::from_be_bytes(dir_buf[base + 8..base + 12].try_into().ok()?);
            let length = u32::from_be_bytes(dir_buf[base + 12..base + 16].try_into().ok()?);
            Some((tag, offset, length))
        })
        .collect();

    // -------------------------------------------------------------------------
    // 3. Seek to and read each required table.
    // -------------------------------------------------------------------------
    let mut collected: Vec<([u8; 4], Vec<u8>)> = Vec::with_capacity(3);

    for &required_tag in &REQUIRED_TAGS {
        // Find the entry in the directory.
        let entry = entries.iter().find(|(tag, _, _)| tag == required_tag);

        if let Some(&(tag, offset, length)) = entry {
            if length == 0 {
                continue;
            }
            file.seek(SeekFrom::Start(u64::from(offset)))
                .map_err(|e| FontError::IoError(Arc::new(e)))?;
            let mut buf = vec![0u8; length as usize];
            file.read_exact(&mut buf)
                .map_err(|e| FontError::IoError(Arc::new(e)))?;
            collected.push((tag, buf));
        }
        // If a required table is absent, skip — parser handles missing tables.
    }

    // -------------------------------------------------------------------------
    // 4. Reconstruct a minimal valid SFNT in memory.
    // -------------------------------------------------------------------------
    let sfnt_bytes = reconstruct_minimal_sfnt(&header[..4], &collected);

    // -------------------------------------------------------------------------
    // 5. Parse the reconstructed SFNT and extract FaceInfo.
    // -------------------------------------------------------------------------
    let arc: Arc<[u8]> = sfnt_bytes.into();
    let count = face_count(&arc);
    let mut faces: Vec<FaceInfo> = Vec::with_capacity(count as usize);

    for idx in 0..count {
        if let Ok(parsed) = ParsedFace::parse(arc.clone(), idx) {
            let mut info = parsed.as_face_info();
            info.path = path.to_path_buf();
            info.face_index = idx;
            faces.push(info);
        }
    }

    if faces.is_empty() {
        return Err(FontError::ParseError(
            "no parseable faces in partial SFNT".to_string(),
        ));
    }

    Ok(faces)
}

/// Reconstruct a minimal SFNT byte buffer from the original `sfnt_version`
/// header bytes and a list of (tag, payload) table pairs.
///
/// The resulting buffer is a fully-conformant SFNT that a spec-compliant
/// parser can parse, containing only the provided tables.
fn reconstruct_minimal_sfnt(sfnt_version: &[u8], tables: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
    let n = tables.len() as u16;

    // Compute SFNT header search fields.
    let (search_range, entry_selector, range_shift) = sfnt_search_fields(n);

    // Layout:
    //   12 bytes  : SFNT header
    //   n × 16    : table records
    //   padded payloads
    let header_size = 12 + n as usize * 16;
    let total_payload: usize = tables.iter().map(|(_, p)| pad4(p.len())).sum();
    let mut out = Vec::with_capacity(header_size + total_payload);

    // SFNT header.
    out.extend_from_slice(sfnt_version);
    out.extend_from_slice(&n.to_be_bytes());
    out.extend_from_slice(&search_range.to_be_bytes());
    out.extend_from_slice(&entry_selector.to_be_bytes());
    out.extend_from_slice(&range_shift.to_be_bytes());

    // Compute table offsets (tables start after the directory).
    let mut offsets: Vec<u32> = Vec::with_capacity(tables.len());
    let mut current_offset = header_size as u32;
    for (_, payload) in tables {
        offsets.push(current_offset);
        current_offset += pad4(payload.len()) as u32;
    }

    // Write directory entries.
    for (i, (tag, payload)) in tables.iter().enumerate() {
        let checksum = sfnt_checksum(payload);
        out.extend_from_slice(tag);
        out.extend_from_slice(&checksum.to_be_bytes());
        out.extend_from_slice(&offsets[i].to_be_bytes());
        out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    }

    // Write padded table payloads.
    for (_, payload) in tables {
        out.extend_from_slice(payload);
        let pad = pad4(payload.len()) - payload.len();
        out.extend(std::iter::repeat_n(0u8, pad));
    }

    out
}

/// Compute the SFNT table directory search fields from the number of tables.
fn sfnt_search_fields(num_tables: u16) -> (u16, u16, u16) {
    let n = num_tables as u32;
    // Largest power of 2 ≤ n.
    let mut p2 = 1u32;
    while p2 * 2 <= n {
        p2 *= 2;
    }
    let search_range = (p2 * 16) as u16;
    let entry_selector = p2.trailing_zeros() as u16;
    let range_shift = ((n - p2) * 16) as u16;
    (search_range, entry_selector, range_shift)
}

/// Compute the SFNT checksum for a table payload (OpenType spec §3.11).
fn sfnt_checksum(data: &[u8]) -> u32 {
    let mut sum: u32 = 0;
    let mut i = 0;
    while i + 3 < data.len() {
        let word = u32::from_be_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
        sum = sum.wrapping_add(word);
        i += 4;
    }
    // Handle trailing bytes (< 4).
    if i < data.len() {
        let mut tail = [0u8; 4];
        tail[..data.len() - i].copy_from_slice(&data[i..]);
        let word = u32::from_be_bytes(tail);
        sum = sum.wrapping_add(word);
    }
    sum
}

/// Round `n` up to the nearest multiple of 4 (SFNT table alignment).
fn pad4(n: usize) -> usize {
    (n + 3) & !3
}

/// Fallback: read the entire file and parse all faces (used for TTC).
fn read_all_faces_full(path: &Path) -> Result<Vec<FaceInfo>, FontError> {
    let bytes = std::fs::read(path)?;
    let arc: Arc<[u8]> = bytes.into();
    let count = face_count(&arc);
    let mut faces: Vec<FaceInfo> = Vec::with_capacity(count as usize);

    for idx in 0..count {
        if let Ok(parsed) = ParsedFace::parse(arc.clone(), idx) {
            let mut info = parsed.as_face_info();
            info.path = path.to_path_buf();
            info.face_index = idx;
            faces.push(info);
        }
    }

    if faces.is_empty() {
        return Err(FontError::ParseError(
            "no parseable faces in font file".to_string(),
        ));
    }

    Ok(faces)
}
