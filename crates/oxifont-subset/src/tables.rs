/// SFNT table directory reading / writing utilities.
use std::borrow::Cow;
use std::collections::HashMap;

/// Error type for all subset operations.
///
/// This enum is `#[non_exhaustive]`: downstream `match` expressions must include
/// a catch-all arm so that new variants can be added in minor versions.
#[derive(Debug)]
#[non_exhaustive]
pub enum SubsetError {
    /// The font data is structurally invalid.
    InvalidFont(String),
    /// A required table is absent.
    TableMissing([u8; 4]),
    /// The requested face index is at or beyond the number of faces the font
    /// data holds (`1` for a plain TTF/OTF, `numFonts` for a `ttcf`
    /// collection).
    ///
    /// Returned by the `*_at_face` entry points and by [`crate::face_count`].
    FaceIndexOutOfRange {
        /// The requested face index.
        index: u32,
        /// The number of faces available.
        count: u32,
    },
    /// A tag in the requested instancing coordinates names no `fvar` axis.
    ///
    /// Returned by [`crate::instance()`]. Ignoring an unknown tag instead would
    /// turn a typo into "the default instance embedded successfully", which is
    /// exactly the failure static instancing exists to prevent.
    UnknownAxis([u8; 4]),
    /// The operation is not implemented for this font's structure.
    ///
    /// The payload names the structure, e.g. a face with no `fvar` axes or with
    /// `CFF`/`CFF2` outlines handed to [`crate::instance()`].
    Unsupported(&'static str),
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
            SubsetError::FaceIndexOutOfRange { index, count } => {
                write!(f, "face index {index} out of range (count={count})")
            }
            SubsetError::UnknownAxis(tag) => {
                write!(
                    f,
                    "no such variation axis: {}",
                    std::str::from_utf8(tag).unwrap_or("????")
                )
            }
            SubsetError::Unsupported(what) => write!(f, "unsupported: {what}"),
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
// Big-endian scalar field accessors
// ---------------------------------------------------------------------------
//
// Shared by every table rewriter and by the instancer, which patch fixed
// offsets inside `head` / `hhea` / `maxp` / `OS/2` copies. `checked_add` keeps a
// hostile offset from wrapping into a panic on the slice range.

/// Read a big-endian `uint16` at `offset`, or `None` when it does not fit.
pub(crate) fn get_u16(data: &[u8], offset: usize) -> Option<u16> {
    let end = offset.checked_add(2)?;
    data.get(offset..end)
        .map(|b| u16::from_be_bytes([b[0], b[1]]))
}

/// Read a big-endian `int16` at `offset`, or `None` when it does not fit.
pub(crate) fn get_i16(data: &[u8], offset: usize) -> Option<i16> {
    let end = offset.checked_add(2)?;
    data.get(offset..end)
        .map(|b| i16::from_be_bytes([b[0], b[1]]))
}

/// Write a big-endian `uint16` at `offset`; a short buffer is left untouched.
pub(crate) fn set_u16(data: &mut [u8], offset: usize, value: u16) {
    if let Some(slot) = offset
        .checked_add(2)
        .and_then(|end| data.get_mut(offset..end))
    {
        slot.copy_from_slice(&value.to_be_bytes());
    }
}

/// Write a big-endian `int16` at `offset`; a short buffer is left untouched.
pub(crate) fn set_i16(data: &mut [u8], offset: usize, value: i16) {
    set_u16(data, offset, value as u16);
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

/// Convert an [`oxifont_core::sfnt::SfntError`] into the crate's error type,
/// preserving the face-index-out-of-range case as its own named variant.
pub(crate) fn map_sfnt_error(e: oxifont_core::sfnt::SfntError) -> SubsetError {
    match e {
        oxifont_core::sfnt::SfntError::FaceIndexOutOfRange { index, count } => {
            SubsetError::FaceIndexOutOfRange { index, count }
        }
        other => SubsetError::InvalidFont(other.to_string()),
    }
}

/// Flatten a parsed [`oxifont_core::sfnt::SfntTableMap`] into the `HashMap`
/// the subsetting pipeline expects.
fn flatten_table_map<'a>(
    sfnt_map: &oxifont_core::sfnt::SfntTableMap<'a>,
) -> HashMap<[u8; 4], &'a [u8]> {
    let mut map = HashMap::with_capacity(sfnt_map.num_tables());
    for tag in sfnt_map.tags() {
        if let Some(slice) = sfnt_map.table(tag) {
            map.insert(*tag, slice);
        }
    }
    map
}

/// Parse an SFNT table directory at offset 0.
///
/// Returns a map from 4-byte tag to the (unpadded) table data slice.
///
/// Delegates to [`oxifont_core::sfnt::SfntTableMap::parse`] for the actual
/// parsing logic, then converts the result into the `HashMap` expected by the
/// subsetting pipeline. A `ttcf` collection is **refused** here, exactly as
/// `SfntTableMap::parse` refuses it — use
/// [`read_table_directory_at_face`] to select a face out of a collection.
///
/// # Errors
/// Returns [`SubsetError::InvalidFont`] if the header is truncated, the magic
/// is not a per-face SFNT magic, or a table record points outside `data`.
pub fn read_table_directory(data: &[u8]) -> Result<HashMap<[u8; 4], &[u8]>, SubsetError> {
    let sfnt_map = oxifont_core::sfnt::SfntTableMap::parse(data).map_err(map_sfnt_error)?;
    Ok(flatten_table_map(&sfnt_map))
}

/// Parse the SFNT table directory of face `face_index`.
///
/// Accepts both a plain per-face SFNT (where the only valid index is `0`, and
/// the result is identical to [`read_table_directory`]) and a `ttcf`
/// collection. Delegates to [`oxifont_core::sfnt::SfntTableMap::parse_face`],
/// which validates the collection header before trusting any offset in it.
///
/// # Errors
/// Returns [`SubsetError::FaceIndexOutOfRange`] when `face_index` is at or
/// beyond the number of faces available, or [`SubsetError::InvalidFont`] for a
/// truncated / malformed container or table directory.
pub fn read_table_directory_at_face(
    data: &[u8],
    face_index: u32,
) -> Result<HashMap<[u8; 4], &[u8]>, SubsetError> {
    let sfnt_map =
        oxifont_core::sfnt::SfntTableMap::parse_face(data, face_index).map_err(map_sfnt_error)?;
    Ok(flatten_table_map(&sfnt_map))
}

// ---------------------------------------------------------------------------
// build_sfnt
// ---------------------------------------------------------------------------

/// TrueType-flavoured sfnt version (`glyf` outlines).
pub const SFNT_VERSION_TRUETYPE: u32 = 0x0001_0000;

/// CFF-flavoured sfnt version (`OTTO`; `CFF ` or `CFF2` outlines).
pub const SFNT_VERSION_CFF: u32 = 0x4F54_544F;

/// Compute the OpenType offset-table binary-search fields for `num_tables`.
///
/// Per the OpenType specification's offset table:
/// `entrySelector = floor(log2(numTables))`,
/// `searchRange = 2^entrySelector * 16`,
/// `rangeShift = numTables * 16 - searchRange`.
///
/// A zero table count yields all zeros (`log2(0)` is undefined). The arithmetic
/// is carried out in `u32` and narrowed with saturation, so a table count above
/// 4095 — which no real font has, and which these `u16` header fields cannot
/// represent — cannot wrap or panic.
fn search_params(num_tables: u16) -> (u16, u16, u16) {
    if num_tables == 0 {
        return (0, 0, 0);
    }
    // floor(log2(n)) for a non-zero u16.
    let entry_selector = 15u32 - num_tables.leading_zeros();
    let search_range = 16u32 << entry_selector;
    let total = u32::from(num_tables) * 16;
    let range_shift = total.saturating_sub(search_range);
    (
        u16::try_from(search_range).unwrap_or(u16::MAX),
        u16::try_from(entry_selector).unwrap_or(u16::MAX),
        u16::try_from(range_shift).unwrap_or(u16::MAX),
    )
}

/// Assemble a new SFNT from a list of `(tag, data)` pairs.
///
/// Tables are sorted by tag (OpenType specification requirement).
/// Correct search params, offsets, and `checkSumAdjustment` in `head` are
/// computed here. The input `tables` should include the `head` table with
/// `checkSumAdjustment` zero — this function patches the final file.
///
/// The sfnt version is chosen from the outline table that is actually present:
/// [`SFNT_VERSION_CFF`] (`OTTO`) when a `CFF ` or `CFF2` table is in the list,
/// [`SFNT_VERSION_TRUETYPE`] (`0x00010000`) otherwise. Consumers dispatch on
/// this field to decide which outline table to read, so a CFF-flavoured subset
/// stamped `0x00010000` would be searched for a `glyf` table that does not
/// exist.
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

    // Outline flavour: CFF/CFF2 → OTTO, otherwise TrueType.
    let sfnt_version = if sorted
        .iter()
        .any(|(tag, _)| *tag == b"CFF " || *tag == b"CFF2")
    {
        SFNT_VERSION_CFF
    } else {
        SFNT_VERSION_TRUETYPE
    };

    // Compute search params over numTables (not numGlyphs).
    let (search_range, entry_selector, range_shift) = search_params(num_tables);

    // Pre-allocate the full output buffer: header (12) + directory (num_tables * 16)
    // + all padded table bodies, to avoid reallocations during assembly.
    let body_size: usize = sorted
        .iter()
        .map(|(_, d)| (d.len() + 3) & !3) // pad each table to 4-byte boundary
        .sum();
    let total_capacity = 12 + sorted.len() * 16 + body_size;

    // Header: sfntVersion (flavour-dependent), then search params.
    let mut out = Vec::with_capacity(total_capacity);
    out.extend_from_slice(&sfnt_version.to_be_bytes());
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
