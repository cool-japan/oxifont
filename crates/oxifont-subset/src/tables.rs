/// SFNT table directory reading / writing utilities.
use std::borrow::Cow;
use std::collections::HashMap;

/// Error type for all subset operations.
#[derive(Debug)]
pub enum SubsetError {
    /// The font data is structurally invalid.
    InvalidFont(String),
    /// A required table is absent.
    TableMissing([u8; 4]),
    /// I/O error (used in tests / file paths).
    Io(std::io::Error),
}

impl std::fmt::Display for SubsetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SubsetError::InvalidFont(msg) => write!(f, "invalid font: {msg}"),
            SubsetError::TableMissing(tag) => {
                write!(
                    f,
                    "required table missing: {}",
                    std::str::from_utf8(tag).unwrap_or("????")
                )
            }
            SubsetError::Io(e) => write!(f, "I/O error: {e}"),
        }
    }
}

impl std::error::Error for SubsetError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SubsetError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for SubsetError {
    fn from(e: std::io::Error) -> Self {
        SubsetError::Io(e)
    }
}

// ---------------------------------------------------------------------------
// Table checksum helper
// ---------------------------------------------------------------------------

/// Compute the OpenType table checksum: sum of all big-endian u32 words
/// (zero-pad to a multiple of 4 bytes).
pub fn table_checksum(data: &[u8]) -> u32 {
    let mut sum: u32 = 0;
    let mut chunks = data.chunks_exact(4);
    for chunk in chunks.by_ref() {
        let word = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        sum = sum.wrapping_add(word);
    }
    // Remaining bytes (< 4) zero-padded.
    let remainder = chunks.remainder();
    if !remainder.is_empty() {
        let mut padded = [0u8; 4];
        padded[..remainder.len()].copy_from_slice(remainder);
        sum = sum.wrapping_add(u32::from_be_bytes(padded));
    }
    sum
}

// ---------------------------------------------------------------------------
// read_table_directory
// ---------------------------------------------------------------------------

/// Parse an SFNT table directory.
///
/// Returns a map from 4-byte tag to the (unpadded) table data slice.
///
/// Delegates to [`oxifont_core::sfnt::SfntTableMap::parse`] for the actual
/// parsing logic, then converts the result into the `HashMap` expected by the
/// subsetting pipeline.
///
/// # Errors
/// Returns [`SubsetError::InvalidFont`] if the header is truncated or
/// a table record points outside `data`.
pub fn read_table_directory(data: &[u8]) -> Result<HashMap<[u8; 4], &[u8]>, SubsetError> {
    let sfnt_map = oxifont_core::sfnt::SfntTableMap::parse(data)
        .map_err(|e| SubsetError::InvalidFont(e.to_string()))?;

    let mut map = HashMap::with_capacity(sfnt_map.num_tables());
    for tag in sfnt_map.tags() {
        if let Some(slice) = sfnt_map.table(tag) {
            map.insert(*tag, slice);
        }
    }
    Ok(map)
}

// ---------------------------------------------------------------------------
// build_sfnt
// ---------------------------------------------------------------------------

/// Assemble a new SFNT from a list of `(tag, data)` pairs.
///
/// Tables are sorted by tag (OpenType specification requirement).
/// Correct search params, offsets, and `checkSumAdjustment` in `head` are
/// computed here. The input `tables` should include the `head` table with
/// `checkSumAdjustment` zero — this function patches the final file.
///
/// Each table's data may be a borrowed slice (no allocation for verbatim copies)
/// or an owned buffer (for rewritten tables).
///
/// Returns the complete SFNT byte buffer.
pub fn build_sfnt(tables: &[([u8; 4], Cow<'_, [u8]>)]) -> Vec<u8> {
    // Sort by tag.
    let mut sorted: Vec<(&[u8; 4], &[u8])> = tables.iter().map(|(t, d)| (t, d.as_ref())).collect();
    sorted.sort_by_key(|(tag, _)| *tag);

    let num_tables = sorted.len() as u16;

    // Compute search params over numTables (not numGlyphs).
    let search_range = 16u16 * num_tables.next_power_of_two() / 2;
    let entry_selector = (num_tables as f64).log2().floor() as u16;
    let range_shift = num_tables * 16 - search_range;

    // Pre-allocate the full output buffer: header (12) + directory (num_tables * 16)
    // + all padded table bodies, to avoid reallocations during assembly.
    let body_size: usize = sorted
        .iter()
        .map(|(_, d)| (d.len() + 3) & !3) // pad each table to 4-byte boundary
        .sum();
    let total_capacity = 12 + sorted.len() * 16 + body_size;

    // Header: sfVersion = TrueType (0x00010000), then search params.
    let mut out = Vec::with_capacity(total_capacity);
    out.extend_from_slice(b"\x00\x01\x00\x00"); // sfVersion
    out.extend_from_slice(&num_tables.to_be_bytes());
    out.extend_from_slice(&search_range.to_be_bytes());
    out.extend_from_slice(&entry_selector.to_be_bytes());
    out.extend_from_slice(&range_shift.to_be_bytes());

    // Table directory — will be filled after we know offsets.
    let dir_start = out.len();
    out.resize(dir_start + (num_tables as usize) * 16, 0);

    // Pad header+directory to 4-byte alignment if needed (already multiple of 4
    // since 12 + n*16 is always divisible by 4).
    let data_start = out.len();

    // Write table data and record offsets.
    let mut offsets: Vec<u32> = Vec::with_capacity(sorted.len());
    for (_, data) in &sorted {
        let aligned_start = out.len() as u32;
        offsets.push(aligned_start);
        out.extend_from_slice(data);
        // Pad to 4-byte boundary.
        while out.len() % 4 != 0 {
            out.push(0);
        }
    }
    let _ = data_start; // suppress unused warning

    // Fill directory entries.
    for (i, ((tag, data), &offset)) in sorted.iter().zip(offsets.iter()).enumerate() {
        let base = dir_start + i * 16;
        // checksum computed with original data (before padding).
        let cs = table_checksum(data);
        out[base..base + 4].copy_from_slice(*tag);
        out[base + 4..base + 8].copy_from_slice(&cs.to_be_bytes());
        out[base + 8..base + 12].copy_from_slice(&offset.to_be_bytes());
        let length = data.len() as u32;
        out[base + 12..base + 16].copy_from_slice(&length.to_be_bytes());
    }

    // Patch head.checkSumAdjustment.
    // Find the head table's data start inside `out`.
    if let Some(head_idx) = sorted.iter().position(|(tag, _)| *tag == b"head") {
        let head_offset = offsets[head_idx] as usize;
        // Compute whole-file checksum.
        let whole = table_checksum(&out);
        let adjustment = 0xB1B0AFBAu32.wrapping_sub(whole);
        // checkSumAdjustment is at byte offset 8 inside the head table.
        let cs_offset = head_offset + 8;
        if cs_offset + 4 <= out.len() {
            out[cs_offset..cs_offset + 4].copy_from_slice(&adjustment.to_be_bytes());
        }
    }

    out
}
