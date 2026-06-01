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
//! `SfntTableMap` operates on a **single per-face SFNT** byte slice. For TTC
//! files you must pre-slice to the per-face SFNT before calling `parse`.
//! See [`ParsedFace::with_table_map`](crate) for an example of how to do this.
use alloc::collections::BTreeMap;

/// Error type for SFNT parsing failures.
#[derive(Debug, Clone, PartialEq)]
pub enum SfntError {
    /// The data buffer is too short to contain a valid SFNT header or directory.
    Truncated,
    /// The SFNT version/magic field is not a recognized per-face SFNT value.
    ///
    /// Note: `0x74746366` ("ttcf") is intentionally rejected — TTC containers
    /// must be pre-sliced to a per-face SFNT before calling `parse`.
    BadMagic(u32),
    /// A table tag appears more than once in the directory.
    DuplicateTag([u8; 4]),
    /// A table entry's `offset + length` extends beyond the data buffer.
    OutOfBounds([u8; 4]),
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
        }
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
    ///   rejected here; pre-slice to the per-face SFNT first.
    /// - [`SfntError::DuplicateTag`] when a tag appears more than once.
    /// - [`SfntError::OutOfBounds`] when a table entry points outside `data`.
    pub fn parse(data: &'a [u8]) -> Result<Self, SfntError> {
        Self::parse_at_offset(data, 0)
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
        // excluded: callers must provide the per-face SFNT offset.
        match sfnt_version {
            0x00010000 // TrueType / TTF
            | 0x4F54544F // CFF / OpenType (OTTO)
            | 0x74727565 // Apple 'true'
            | 0x74797031 // Apple 'typ1'
            => {}
            _ => return Err(SfntError::BadMagic(sfnt_version)),
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
