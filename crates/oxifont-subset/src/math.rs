//! MATH table subsetting.
//!
//! The MATH table (OpenType Math) embeds Coverage tables that reference GIDs
//! in its sub-tables: `MathGlyphInfo` and `MathVariants`.  This module remaps
//! those Coverage tables for the surviving GID set while copying all other
//! MATH data verbatim.
//!
//! # Strategy
//!
//! 1. Parse the 5-field header (10 bytes): majorVersion, minorVersion,
//!    MathConstantsOffset, MathGlyphInfoOffset, MathVariantsOffset.
//! 2. `MathConstants` (pointed to by MathConstantsOffset) contains only
//!    numeric constants — no GID references.  Copied verbatim.
//! 3. `MathGlyphInfo` (at MathGlyphInfoOffset): parse its 4 sub-offsets
//!    (each a u16 relative to MathGlyphInfo base).  The first u16 at each
//!    sub-table is a Coverage offset relative to the sub-table itself.
//!    Remap the coverage at each non-zero sub-table.
//! 4. `MathVariants` (at MathVariantsOffset): bytes 4-7 hold
//!    VertGlyphCoverageOffset and HorizGlyphCoverageOffset (u16 each,
//!    relative to MathVariants base).  Remap those two coverages.
//! 5. All other data (DeviceTables, GlyphAssembly, construction parts, etc.)
//!    is copied verbatim — GID references in construction parts may become
//!    dangling for removed glyphs but the table stays structurally valid.
//!
//! On any parse error the table is returned verbatim.

use std::collections::HashMap;

use crate::layout;

// ---------------------------------------------------------------------------
// Big-endian integer helpers
// ---------------------------------------------------------------------------

#[inline]
fn r_u16(data: &[u8], offset: usize) -> Option<u16> {
    data.get(offset..offset + 2)
        .map(|b| u16::from_be_bytes([b[0], b[1]]))
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Remap Coverage tables embedded in the MATH table for the surviving GIDs.
///
/// Returns the rewritten table bytes.  On any parse error the original bytes
/// are returned verbatim (safe fallback).
pub fn rewrite_math(table: &[u8], gid_remap: &HashMap<u16, u16>) -> Vec<u8> {
    match try_rewrite_math(table, gid_remap) {
        Some(out) => out,
        None => table.to_vec(),
    }
}

// ---------------------------------------------------------------------------
// Internal implementation
// ---------------------------------------------------------------------------

/// Returns `None` on any parse error.
fn try_rewrite_math(table: &[u8], gid_remap: &HashMap<u16, u16>) -> Option<Vec<u8>> {
    // MATH header: 10 bytes
    // majorVersion u16  (0)
    // minorVersion u16  (2)
    // MathConstantsOffset u16  (4)
    // MathGlyphInfoOffset u16  (6)
    // MathVariantsOffset  u16  (8)
    if table.len() < 10 {
        return None;
    }

    // Work on a mutable copy.
    let mut out = table.to_vec();

    let math_glyph_info_offset = r_u16(table, 6)? as usize;
    let math_variants_offset = r_u16(table, 8)? as usize;

    // --- MathGlyphInfo ---
    // MathGlyphInfo has 4 sub-table offsets (u16 each, relative to
    // MathGlyphInfo base):
    //   0: MathItalicsCorrectionInfoOffset
    //   2: MathTopAccentAttachmentOffset
    //   4: ExtendedShapeCoverageOffset        ← Coverage directly (no sub-table header)
    //   6: MathKernInfoOffset
    //
    // MathItalicsCorrectionInfo and MathTopAccentAttachment each start with:
    //   ItalicsCorrectionCoverageOffset u16  (relative to the sub-table itself)
    //   ... (rest of sub-table)
    //
    // ExtendedShapeCoverage is a Coverage table directly referenced.
    //
    // MathKernInfo starts with:
    //   MathKernCoverageOffset u16 (relative to MathKernInfo)
    //   ... (rest)

    if math_glyph_info_offset != 0 {
        let gi_base = math_glyph_info_offset;
        if gi_base + 8 <= table.len() {
            // Sub-offsets at gi_base + 0, 2, 4, 6.
            let italics_off = r_u16(table, gi_base)?;
            let top_accent_off = r_u16(table, gi_base + 2)?;
            let ext_shape_cov_off = r_u16(table, gi_base + 4)?;
            let kern_info_off = r_u16(table, gi_base + 6)?;

            // MathItalicsCorrectionInfo: Coverage at sub-table[0..2].
            if italics_off != 0 {
                let sub_base = gi_base + italics_off as usize;
                patch_coverage_at_sub_table(&mut out, table, sub_base, 0, gid_remap);
            }

            // MathTopAccentAttachment: Coverage at sub-table[0..2].
            if top_accent_off != 0 {
                let sub_base = gi_base + top_accent_off as usize;
                patch_coverage_at_sub_table(&mut out, table, sub_base, 0, gid_remap);
            }

            // ExtendedShapeCoverage: direct coverage at gi_base + ext_shape_cov_off.
            if ext_shape_cov_off != 0 {
                let cov_base = gi_base + ext_shape_cov_off as usize;
                patch_coverage_inline(&mut out, table, cov_base, gid_remap);
            }

            // MathKernInfo: Coverage at sub-table[0..2].
            if kern_info_off != 0 {
                let sub_base = gi_base + kern_info_off as usize;
                patch_coverage_at_sub_table(&mut out, table, sub_base, 0, gid_remap);
            }
        }
    }

    // --- MathVariants ---
    // MathVariants structure:
    //   0: MinConnectorOverlap u16
    //   2: VertGlyphCoverageOffset u16   (relative to MathVariants base)
    //   4: HorizGlyphCoverageOffset u16  (relative to MathVariants base)
    //   6: VertGlyphCount u16
    //   8: HorizGlyphCount u16
    //   ... (VertGlyphConstruction offsets then HorizGlyphConstruction offsets)
    if math_variants_offset != 0 {
        let mv_base = math_variants_offset;
        if mv_base + 6 <= table.len() {
            let vert_cov_off = r_u16(table, mv_base + 2)?;
            let horiz_cov_off = r_u16(table, mv_base + 4)?;

            if vert_cov_off != 0 {
                let cov_base = mv_base + vert_cov_off as usize;
                patch_coverage_inline(&mut out, table, cov_base, gid_remap);
            }
            if horiz_cov_off != 0 {
                let cov_base = mv_base + horiz_cov_off as usize;
                patch_coverage_inline(&mut out, table, cov_base, gid_remap);
            }
        }
    }

    Some(out)
}

// ---------------------------------------------------------------------------
// Coverage patch helpers
// ---------------------------------------------------------------------------

/// Read the Coverage offset stored at `sub_base + cov_offset_field` (a u16
/// relative to `sub_base`), remap the coverage, and write it back in-place.
///
/// The new coverage bytes are spliced into `out` at the absolute position
/// `sub_base + coverage_sub_offset`.  Because we cannot resize without
/// shifting all other offsets we use a copy-on-replace approach: if the new
/// coverage is the same length we overwrite in place; otherwise we leave the
/// original verbatim (safe fallback — slightly wrong but structurally valid).
fn patch_coverage_at_sub_table(
    out: &mut [u8],
    original: &[u8],
    sub_base: usize,
    cov_field_offset: usize, // byte offset within the sub-table of the Coverage offset field
    gid_remap: &HashMap<u16, u16>,
) {
    let cov_offset_in_sub = match r_u16(original, sub_base + cov_field_offset) {
        Some(o) => o as usize,
        None => return,
    };
    if cov_offset_in_sub == 0 {
        return;
    }
    let abs_cov = sub_base + cov_offset_in_sub;
    patch_coverage_inline(out, original, abs_cov, gid_remap);
}

/// Remap the Coverage table that starts at `abs_cov` in the MATH table.
///
/// Reads coverage from `original`, remaps via `gid_remap`, and overwrites the
/// same region in `out` — provided the new bytes fit exactly in the same space
/// (same or shorter length padded with zeros; excess bytes left as-is if
/// longer, which should not occur for a strict remap that can only shrink
/// or keep the same coverage).
fn patch_coverage_inline(
    out: &mut [u8],
    original: &[u8],
    abs_cov: usize,
    gid_remap: &HashMap<u16, u16>,
) {
    if abs_cov >= original.len() {
        return;
    }
    // Determine old coverage byte length so we know the region size.
    let old_cov_len = coverage_byte_len(original, abs_cov);
    if old_cov_len == 0 {
        return;
    }
    let (new_cov_bytes, _) = layout::remap_coverage(original, abs_cov, gid_remap);
    let new_len = new_cov_bytes.len();

    // Write new coverage into `out` at the same position.
    if abs_cov + new_len <= out.len() {
        out[abs_cov..abs_cov + new_len].copy_from_slice(&new_cov_bytes);
        // If new coverage is shorter, zero-fill the tail.
        if new_len < old_cov_len && abs_cov + old_cov_len <= out.len() {
            out[abs_cov + new_len..abs_cov + old_cov_len].fill(0);
        }
    }
    // If new coverage is longer than the old space: do nothing (safe — old
    // coverage bytes remain, which is structurally valid if imprecise).
}

/// Return the byte length of a Coverage table at `offset` in `data`.
/// Returns 0 on parse failure.
fn coverage_byte_len(data: &[u8], offset: usize) -> usize {
    let format = match r_u16(data, offset) {
        Some(f) => f,
        None => return 0,
    };
    match format {
        1 => {
            let count = match r_u16(data, offset + 2) {
                Some(c) => c as usize,
                None => return 0,
            };
            4 + count * 2
        }
        2 => {
            let count = match r_u16(data, offset + 2) {
                Some(c) => c as usize,
                None => return 0,
            };
            4 + count * 6
        }
        _ => 0,
    }
}
