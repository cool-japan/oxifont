//! Zero-copy SFNT table directory parser shared across the oxifont workspace.
//!
//! `SfntTableMap<'a>` parses the 12-byte SFNT header and all 16-byte directory
//! entries from a raw per-face SFNT byte slice. It returns zero-copy `&'a [u8]`
//! slices for each table via a `BTreeMap`, providing sorted tag iteration with
//! zero extra allocations beyond the map itself.
//!
//! # Usage
//!
//! ```no_run
//! use oxifont_core::sfnt::{SfntTableMap, SfntError};
//!
//! let font_bytes: Vec<u8> = std::fs::read("font.ttf").unwrap();
//! let map = SfntTableMap::parse(&font_bytes).expect("must parse");
//! if let Some(glyf) = map.table(b"glyf") {
//!     println!("glyf table: {} bytes", glyf.len());
//! }
//! ```
//!
//! # TTC (TrueType Collections)
//!
//! [`SfntTableMap::parse`] operates on a **single per-face SFNT** byte slice
//! and rejects the `ttcf` container magic. To read a face out of a collection,
//! either call [`SfntTableMap::parse_face`] with the face index (the
//! `ttf-parser` `Face::parse(data, index)` shape) or resolve the offset
//! yourself with [`face_offset`] and hand it to
//! [`SfntTableMap::parse_at_offset`]. [`face_count`] reports how many faces a
//! buffer holds (`1` for a plain TTF/OTF).
//!
//! ```no_run
//! use oxifont_core::sfnt::{face_count, SfntTableMap};
//!
//! let bytes: Vec<u8> = std::fs::read("msgothic.ttc").unwrap();
//! for index in 0..face_count(&bytes).unwrap() {
//!     let map = SfntTableMap::parse_face(&bytes, index).unwrap();
//!     println!("face {index}: {} tables", map.num_tables());
//! }
//! ```
use alloc::collections::BTreeMap;

/// Error type for SFNT parsing failures.
///
/// This enum is `#[non_exhaustive]`: downstream `match` expressions must include
/// a catch-all arm so that new error variants can be added in minor versions.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum SfntError {
    /// The data buffer is too short to contain a valid SFNT header or directory.
    Truncated,
    /// The SFNT version/magic field is not a recognized per-face SFNT value.
    ///
    /// Note: `0x74746366` ("ttcf") is intentionally rejected by
    /// [`SfntTableMap::parse`] — use [`SfntTableMap::parse_face`] (or resolve
    /// the offset with [`face_offset`]) to read a face out of a collection.
    BadMagic(u32),
    /// A table tag appears more than once in the directory.
    DuplicateTag([u8; 4]),
    /// A table entry's `offset + length` extends beyond the data buffer.
    OutOfBounds([u8; 4]),
    /// The `ttcf` collection header is not trustworthy: its major version is
    /// neither 1 nor 2, or it declares zero faces.
    MalformedCollection,
    /// The requested face index is at or beyond the number of faces the buffer
    /// holds (`1` for a plain per-face SFNT).
    FaceIndexOutOfRange {
        /// The requested face index.
        index: u32,
        /// The number of faces available.
        count: u32,
    },
}

impl core::fmt::Display for SfntError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SfntError::Truncated => write!(f, "SFNT data truncated"),
            SfntError::BadMagic(m) => write!(f, "bad SFNT magic: {:#010x}", m),
            SfntError::DuplicateTag(t) => {
                let s = core::str::from_utf8(t).unwrap_or("????");
                write!(f, "duplicate table tag: {}", s)
            }
            SfntError::OutOfBounds(t) => {
                let s = core::str::from_utf8(t).unwrap_or("????");
                write!(f, "table out of bounds: {}", s)
            }
            SfntError::MalformedCollection => write!(f, "malformed ttcf collection header"),
            SfntError::FaceIndexOutOfRange { index, count } => {
                write!(f, "face index {} out of range (count={})", index, count)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// TTC (TrueType Collection) container
// ---------------------------------------------------------------------------

/// The `ttcf` TrueType Collection container magic (`0x74746366`).
pub const TTC_MAGIC: u32 = 0x7474_6366;

/// Returns `true` for an sfnt version that introduces a single per-face SFNT.
fn is_per_face_magic(sfnt_version: u32) -> bool {
    matches!(
        sfnt_version,
        0x0001_0000 // TrueType / TTF
            | 0x4F54_544F // CFF / OpenType (OTTO)
            | 0x7472_7565 // Apple 'true'
            | 0x7479_7031 // Apple 'typ1'
    )
}

/// Read the leading 4-byte magic of `data`.
fn leading_magic(data: &[u8]) -> Result<u32, SfntError> {
    let m = data.get(0..4).ok_or(SfntError::Truncated)?;
    Ok(u32::from_be_bytes([m[0], m[1], m[2], m[3]]))
}

/// Validate a `ttcf` header and return its declared `numFonts`.
///
/// Every field the offset table's size depends on is checked before it is
/// used, so a hostile header (zero or absurd `numFonts`, unknown version,
/// truncated offset table) is refused rather than trusted.
fn collection_face_count(data: &[u8]) -> Result<u32, SfntError> {
    let header = data.get(0..12).ok_or(SfntError::Truncated)?;
    // Only TTC header versions 1.0 and 2.0 are defined.
    let major_version = u16::from_be_bytes([header[4], header[5]]);
    if major_version != 1 && major_version != 2 {
        return Err(SfntError::MalformedCollection);
    }
    let num_fonts = u32::from_be_bytes([header[8], header[9], header[10], header[11]]);
    if num_fonts == 0 {
        return Err(SfntError::MalformedCollection);
    }
    // The offset table follows the 12-byte header: one Offset32 per face.
    let offsets_len = (num_fonts as usize)
        .checked_mul(4)
        .ok_or(SfntError::Truncated)?;
    let needed = offsets_len.checked_add(12).ok_or(SfntError::Truncated)?;
    if data.len() < needed {
        return Err(SfntError::Truncated);
    }
    Ok(num_fonts)
}

/// Returns the number of faces `data` holds.
///
/// A plain per-face SFNT (TTF/OTF) holds exactly one face; a `ttcf`
/// collection holds its declared `numFonts`.
///
/// # Errors
///
/// - [`SfntError::Truncated`] when `data` is too short for the magic or, for a
///   collection, for the header plus its offset table.
/// - [`SfntError::BadMagic`] when `data` is neither a recognised per-face SFNT
///   nor a `ttcf` collection.
/// - [`SfntError::MalformedCollection`] when the collection header declares an
///   unknown version or zero faces.
pub fn face_count(data: &[u8]) -> Result<u32, SfntError> {
    let magic = leading_magic(data)?;
    if magic == TTC_MAGIC {
        collection_face_count(data)
    } else if is_per_face_magic(magic) {
        Ok(1)
    } else {
        Err(SfntError::BadMagic(magic))
    }
}

/// Returns the byte offset of face `face_index`'s SFNT header within `data`.
///
/// For a plain per-face SFNT the only valid index is `0`, which resolves to
/// offset `0`. The returned offset is taken verbatim from the collection's
/// offset table and is **not** yet known to point at an SFNT header —
/// [`SfntTableMap::parse_at_offset`] performs that check.
///
/// # Errors
///
/// As [`face_count`], plus [`SfntError::FaceIndexOutOfRange`] when
/// `face_index` is at or beyond the number of faces available.
pub fn face_offset(data: &[u8], face_index: u32) -> Result<usize, SfntError> {
    let magic = leading_magic(data)?;
    if magic == TTC_MAGIC {
        let count = collection_face_count(data)?;
        if face_index >= count {
            return Err(SfntError::FaceIndexOutOfRange {
                index: face_index,
                count,
            });
        }
        // Bounds guaranteed by `collection_face_count`, but read defensively.
        let record_start = 12 + (face_index as usize) * 4;
        let record = data
            .get(record_start..record_start + 4)
            .ok_or(SfntError::Truncated)?;
        Ok(u32::from_be_bytes([record[0], record[1], record[2], record[3]]) as usize)
    } else if is_per_face_magic(magic) {
        if face_index != 0 {
            return Err(SfntError::FaceIndexOutOfRange {
                index: face_index,
                count: 1,
            });
        }
        Ok(0)
    } else {
        Err(SfntError::BadMagic(magic))
    }
}

/// Zero-copy view of a single per-face SFNT font's table directory.
///
/// Parsed from raw per-face SFNT bytes. All table data slices borrow from the
/// original `data` passed to [`SfntTableMap::parse`] — no extra heap
/// allocations for table data beyond the `BTreeMap` itself.
///
/// For TTC containers, pre-slice to the per-face SFNT offset before calling
/// [`SfntTableMap::parse`].
#[derive(Debug)]
pub struct SfntTableMap<'a> {
    /// The SFNT version field.
    ///
    /// Common values:
    /// - `0x00010000`: TrueType / plain TTF
    /// - `0x4F54544F` (`OTTO`): CFF / OpenType with CFF outlines
    /// - `0x74727565` (`true`): Apple TrueType variant
    /// - `0x74797031` (`typ1`): Apple Type 1 variant
    pub sfnt_version: u32,
    /// Map from 4-byte tag to the table's raw bytes (zero-copy into `raw`).
    tables: BTreeMap<[u8; 4], &'a [u8]>,
    /// The original raw per-face SFNT bytes.
    raw: &'a [u8],
}

impl<'a> SfntTableMap<'a> {
    /// Parse the SFNT table directory from a raw per-face SFNT byte slice.
    ///
    /// Validates the magic bytes, reads the 12-byte header and all 16-byte
    /// directory entries. Returns slices into `data` — zero allocations beyond
    /// the `BTreeMap`.
    ///
    /// # Errors
    ///
    /// - [`SfntError::Truncated`] when `data` is shorter than the full header
    ///   plus directory.
    /// - [`SfntError::BadMagic`] when the first four bytes are not a recognised
    ///   per-face SFNT magic. Note: `0x74746366` ("ttcf") is intentionally
    ///   rejected here; use [`parse_face`](Self::parse_face) to select a face
    ///   out of a collection.
    /// - [`SfntError::DuplicateTag`] when a tag appears more than once.
    /// - [`SfntError::OutOfBounds`] when a table entry points outside `data`.
    pub fn parse(data: &'a [u8]) -> Result<Self, SfntError> {
        Self::parse_at_offset(data, 0)
    }

    /// Parse the table directory of face `face_index`.
    ///
    /// Accepts both a plain per-face SFNT (where the only valid index is `0`)
    /// and a `ttcf` collection, mirroring `ttf_parser::Face::parse(data,
    /// index)`. The face's offset is resolved with [`face_offset`] and the
    /// SFNT header at that offset is validated exactly as
    /// [`parse`](Self::parse) validates offset 0, so a collection whose offset
    /// table points past the end of the buffer — or at bytes that are not an
    /// SFNT header — is refused rather than trusted.
    ///
    /// # Errors
    ///
    /// As [`parse`](Self::parse), plus [`SfntError::MalformedCollection`] and
    /// [`SfntError::FaceIndexOutOfRange`] from [`face_offset`].
    pub fn parse_face(data: &'a [u8], face_index: u32) -> Result<Self, SfntError> {
        let offset = face_offset(data, face_index)?;
        Self::parse_at_offset(data, offset)
    }

    /// Parse the SFNT table directory for a face embedded within a TTC file.
    ///
    /// The SFNT header is read starting at `sfnt_offset` within `data`, but
    /// table data offsets in the directory records are interpreted as **absolute
    /// offsets from the start of `data`** — exactly as the OpenType spec
    /// requires for TTC-embedded SFNTs.
    ///
    /// For plain TTF/OTF files use [`parse`](Self::parse) (i.e. `sfnt_offset = 0`).
    ///
    /// # Errors
    ///
    /// Same as [`parse`](Self::parse).
    pub fn parse_at_offset(data: &'a [u8], sfnt_offset: usize) -> Result<Self, SfntError> {
        // Need at least the 12-byte SFNT header starting at sfnt_offset.
        let header_end = sfnt_offset.checked_add(12).ok_or(SfntError::Truncated)?;
        if data.len() < header_end {
            return Err(SfntError::Truncated);
        }

        let h = &data[sfnt_offset..sfnt_offset + 12];
        let sfnt_version = u32::from_be_bytes([h[0], h[1], h[2], h[3]]);

        // Validate magic — TTC header ("ttcf" = 0x74746366) is intentionally
        // excluded: callers must provide the per-face SFNT offset (see
        // `parse_face` / `face_offset`).
        if !is_per_face_magic(sfnt_version) {
            return Err(SfntError::BadMagic(sfnt_version));
        }

        let num_tables = u16::from_be_bytes([h[4], h[5]]) as usize;

        // Directory occupies [sfnt_offset + 12 .. sfnt_offset + 12 + num_tables * 16].
        let dir_size = num_tables.checked_mul(16).ok_or(SfntError::Truncated)?;
        let dir_start = sfnt_offset.checked_add(12).ok_or(SfntError::Truncated)?;
        let dir_end = dir_start
            .checked_add(dir_size)
            .ok_or(SfntError::Truncated)?;
        if data.len() < dir_end {
            return Err(SfntError::Truncated);
        }

        let mut tables: BTreeMap<[u8; 4], &'a [u8]> = BTreeMap::new();

        for i in 0..num_tables {
            let entry_start = dir_start + i * 16;
            let entry = &data[entry_start..entry_start + 16];

            // Tag is the first four bytes.
            let tag = [entry[0], entry[1], entry[2], entry[3]];

            // checksum bytes [4..8] — not validated here.

            // For TTC-embedded SFNTs the offset is absolute from the start of `data`.
            let offset = u32::from_be_bytes([entry[8], entry[9], entry[10], entry[11]]) as usize;
            let length = u32::from_be_bytes([entry[12], entry[13], entry[14], entry[15]]) as usize;

            let end = offset
                .checked_add(length)
                .ok_or(SfntError::OutOfBounds(tag))?;
            if end > data.len() {
                return Err(SfntError::OutOfBounds(tag));
            }

            if tables.insert(tag, &data[offset..end]).is_some() {
                return Err(SfntError::DuplicateTag(tag));
            }
        }

        // `raw` stores the full slice from sfnt_offset to include the directory
        // and all reachable table data. We use the full `data` slice so callers
        // can call `raw()` to get bytes that feed directly into `subset_with_gid_set`.
        Ok(SfntTableMap {
            sfnt_version,
            tables,
            raw: data,
        })
    }

    /// Returns the raw bytes of a table by its 4-byte tag, or `None` if absent.
    ///
    /// The returned slice borrows from the original data passed to [`parse`](Self::parse).
    pub fn table(&self, tag: &[u8; 4]) -> Option<&'a [u8]> {
        self.tables.get(tag).copied()
    }

    /// Returns an iterator over all table tags in sorted (BTreeMap) order.
    pub fn tags(&self) -> impl Iterator<Item = &[u8; 4]> {
        self.tables.keys()
    }

    /// Returns the original raw per-face SFNT bytes.
    pub fn raw(&self) -> &'a [u8] {
        self.raw
    }

    /// Returns the number of tables in the directory.
    pub fn num_tables(&self) -> usize {
        self.tables.len()
    }
}
