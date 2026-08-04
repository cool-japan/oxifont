#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(clippy::all)]

//! `oxifont-subset` — Pure Rust OpenType font subsetter.
//!
//! Takes raw SFNT font bytes and a set of Unicode codepoints, and produces new
//! SFNT bytes containing only the requested glyphs (plus `.notdef`).
//!
//! # Example
//!
//! ```no_run
//! use std::collections::BTreeSet;
//! use oxifont_subset::subset_font;
//!
//! let font_data = std::fs::read("NotoSans-Regular.ttf").unwrap();
//! let cps: BTreeSet<char> = ['A', 'B', 'C'].iter().copied().collect();
//! let subset_bytes = subset_font(&font_data, &cps).expect("subset failed");
//! ```
//!
//! # Entry-point naming
//!
//! The subsetting entry points come in a small matrix. The base names —
//! [`subset_font`], [`subset_font_with_options`], [`subset_by_gids`],
//! [`subset_with_gid_set`] — read a single per-face SFNT at offset 0 and
//! return the subset bytes (plus [`SubsetStats`] where applicable). Two
//! orthogonal suffixes extend them:
//!
//! | Suffix | Effect |
//! |--------|--------|
//! | `_at_face` | Takes a `face_index` after `font_data`, so a `ttcf` collection (every stock Windows CJK font) can be subset without pre-slicing. See [`face_count`]. |
//! | `_mapped` | Returns a [`SubsetGidMap`] as an extra tuple element, recovering the old ↔ new glyph-ID renumbering the subset performed. |
//!
//! [`subset_with_gid_set_at_face_mapped`] combines both. The base entry points
//! reject a `ttcf` container rather than silently picking face 0 — a
//! collection's faces are different fonts, so which one to subset is the
//! caller's decision.

/// CBDT/CBLC color bitmap table subsetting.
pub mod cbdt;
/// CFF (Compact Font Format) table subsetting.
pub mod cff;
/// `cmap` table rewriting utilities.
pub mod cmap;
/// COLR table v0 subsetting: remap base/layer GIDs and drop removed records.
pub mod colr;
/// Old ↔ new glyph-ID mapping produced by a subset operation.
pub mod gid_map;
/// `glyf` and `loca` table rewriting utilities.
pub mod glyf;
/// `gvar` per-glyph variation data rewriter.
pub mod gvar;
/// Static instancing: pin a variable font at one design location.
pub mod instance;
/// `kern` table pair pruning and GID remapping.
pub mod kern;
/// Coverage, ClassDef, and GDEF layout table helpers.
pub mod layout;
/// MATH table Coverage remapping for mathematical typesetting fonts.
pub mod math;
/// OS/2 table unicode-range and first/last-char rewriter.
pub mod os2;
/// OpenType Layout (OTL) table rewriters: GSUB.
pub mod otl;
/// Contextual / chaining-contextual lookup subtables shared by GSUB and GPOS.
pub(crate) mod otl_context;
/// OpenType Layout GPOS table rewriter.
pub mod otl_gpos;
/// On-the-fly font subsetting for PDF text rendering pipelines.
pub mod pdf_subset;
/// sbix table subsetting: rebuild per-glyph bitmap strike arrays for the new GID space.
pub mod sbix;
/// SVG table subsetting: remove document index entries for removed GIDs.
pub mod svg;
/// SFNT table directory read/write and error types.
pub mod tables;
/// HVAR / VVAR delta-set index map rewriter for variable fonts.
pub mod varfont;

pub use gid_map::SubsetGidMap;
pub use instance::instance;
pub use tables::SubsetError;

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use tables::{get_i16, get_u16, set_i16, set_u16};

// ---------------------------------------------------------------------------
// SubsetOptions
// ---------------------------------------------------------------------------

/// Options controlling the subsetting behaviour.
///
/// Use the builder methods to customise individual fields; all fields have
/// sensible defaults via [`SubsetOptions::default`].
///
/// # Example
///
/// ```no_run
/// use oxifont_subset::SubsetOptions;
///
/// let opts = SubsetOptions::default()
///     .strip_hints(true)
///     .retain_names(false)
///     .drop_variations(true);
/// ```
///
/// This struct is `#[non_exhaustive]`: build it from
/// [`SubsetOptions::default`] and the builder methods rather than with a struct
/// literal, so that a new option can be added without a breaking change.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct SubsetOptions {
    /// Drop `fpgm`, `prep`, and `cvt ` (TrueType hint tables).
    ///
    /// Useful for web fonts where hinting is rarely beneficial.
    pub strip_hints: bool,

    /// Keep the `GSUB`, `GPOS`, and `GDEF` tables, with glyph ID references
    /// remapped to the subset's new GID space (not copied verbatim — stale
    /// GIDs would corrupt shaping).
    ///
    /// Every GSUB lookup type (1–8) and GPOS lookup type (1–9) is remapped and
    /// retained, including contextual / chaining lookups in all three formats
    /// and their Extension-wrapped forms. A subtable is dropped only when it is
    /// malformed or can no longer match anything under the subset (for example
    /// a context whose input coverage emptied out); see
    /// [`SubsetStats::dropped_context_subtables`] to detect the latter.
    ///
    /// Still not rewritten: GSUB/GPOS v1.1 `FeatureVariations`, which is
    /// dropped along with the minor version (the rebuilt table is v1.0).
    ///
    /// Set to `false` if you want to strip layout tables entirely (reduces
    /// file size but disables OpenType shaping features).
    pub retain_layout_tables: bool,

    /// Keep the full `name` table.
    ///
    /// When `false`, only name IDs 0–6 are retained.
    pub retain_names: bool,

    /// If set, only include GIDs whose mapped codepoint falls within
    /// `[lo, hi]` (inclusive on both ends) during the cmap scan.
    ///
    /// Codepoints in the requested set that fall outside this range are
    /// silently dropped.
    pub retain_codepoint_range: Option<(char, char)>,

    /// Emit a **static** font: drop `fvar`, `avar`, `gvar`, `cvar`, `HVAR`,
    /// `VVAR`, `MVAR` and `STAT`.
    ///
    /// The retained `glyf` outlines and `hmtx` advances are then the font's
    /// **default master** — which is the correct static font only if the
    /// default location is what you wanted. To pin a different location, call
    /// [`instance()`](crate::instance()) first; its output has no variation
    /// tables and this flag is then a no-op.
    ///
    /// Leaving this `false` on a variable face produces a subset that still
    /// advertises axes: 2.6× larger on a stock two-axis UI font, and a program
    /// whose outlines are the default master while `fvar` claims otherwise.
    pub drop_variations: bool,
}

impl Default for SubsetOptions {
    fn default() -> Self {
        Self {
            strip_hints: false,
            retain_layout_tables: true,
            retain_names: true,
            retain_codepoint_range: None,
            drop_variations: false,
        }
    }
}

impl SubsetOptions {
    /// Set whether TrueType hint tables (`fpgm`, `prep`, `cvt `) are dropped.
    #[must_use]
    pub fn strip_hints(mut self, v: bool) -> Self {
        self.strip_hints = v;
        self
    }

    /// Set whether layout tables (`GSUB`, `GPOS`, `GDEF`) are retained (with
    /// GID references remapped) rather than dropped entirely. See
    /// [`SubsetOptions::retain_layout_tables`] for exactly what is remapped and
    /// what is not.
    #[must_use]
    pub fn retain_layout_tables(mut self, v: bool) -> Self {
        self.retain_layout_tables = v;
        self
    }

    /// Set whether the full `name` table is retained.
    #[must_use]
    pub fn retain_names(mut self, v: bool) -> Self {
        self.retain_names = v;
        self
    }

    /// Restrict the cmap scan to codepoints within `[lo, hi]` (inclusive).
    ///
    /// Codepoints outside the range are not included even if they appear in
    /// the requested `codepoints` set.
    #[must_use]
    pub fn retain_codepoint_range(mut self, lo: char, hi: char) -> Self {
        self.retain_codepoint_range = Some((lo, hi));
        self
    }

    /// Set whether the variation tables (`fvar`, `avar`, `gvar`, `cvar`,
    /// `HVAR`, `VVAR`, `MVAR`, `STAT`) are dropped, producing a static font at
    /// the source's default master. See
    /// [`SubsetOptions::drop_variations`].
    #[must_use]
    pub fn drop_variations(mut self, v: bool) -> Self {
        self.drop_variations = v;
        self
    }
}

// ---------------------------------------------------------------------------
// SubsetStats
// ---------------------------------------------------------------------------

/// Statistics produced by a subset operation.
#[derive(Debug, Clone)]
pub struct SubsetStats {
    /// Size of the original font in bytes.
    pub original_size: usize,
    /// Size of the subset font in bytes.
    pub subset_size: usize,
    /// Number of glyphs in the subset font (including `.notdef`).
    pub glyphs_retained: u16,
    /// 4-byte tags of all tables present in the subset font.
    pub tables_retained: Vec<[u8; 4]>,
    /// Number of advanced-layout subtables dropped during GSUB/GPOS rewriting:
    /// GSUB types 5, 6 and 8 and GPOS types 3, 5, 7 and 8.
    ///
    /// All three contextual formats (glyph rule sets, class rule sets and
    /// coverage-based) are remapped, so a non-zero count no longer means an
    /// unsupported format was skipped. It means the subtable either was
    /// malformed or can no longer match anything under this subset — e.g. a
    /// context position whose coverage emptied out, or a cursive / mark
    /// attachment whose glyphs all left the subset. Those are legitimate
    /// prunes; the counter exists so callers can notice how much contextual
    /// machinery their glyph set discarded.
    ///
    /// Extension-wrapped lookups (GSUB 7 / GPOS 9) are counted under the outer
    /// Extension type, which is not in the list above, so a dropped
    /// Extension-wrapped context is not reflected here.
    pub dropped_context_subtables: usize,

    /// The `CFF `/`CFF2` charstrings were copied from the source **verbatim**
    /// instead of being subset.
    ///
    /// The CFF rewriter falls back to a verbatim copy for CID-keyed fonts
    /// (those carrying an `FDSelect`) and for structures it cannot parse. The
    /// resulting table is correct only under the *original* glyph numbering,
    /// while the rest of the subset has been renumbered — so the glyphs render
    /// as the wrong characters, and the table is as large as the source's.
    ///
    /// Callers that embed CFF subsets must check this: when it is `true` the
    /// only safe options are to embed the original face or to refuse. It is
    /// always `false` for a `glyf`-flavoured font.
    pub cff_charstrings_verbatim: bool,
}

// ---------------------------------------------------------------------------
// cmap → GID resolution
// ---------------------------------------------------------------------------

/// Walk a cmap table and build a map from Unicode codepoint (u32) → GID (u16).
///
/// This is the public re-export used by [`pdf_subset::PdfFontSubsetter`].
/// Tries format 12 first (full Unicode), then format 4 (BMP).
pub fn cmap_to_gid_map_pub(cmap_data: &[u8]) -> Result<HashMap<u32, u16>, SubsetError> {
    cmap_to_gid_map(cmap_data)
}

/// Walk a cmap table and build a map from Unicode codepoint (u32) → GID (u16).
///
/// Tries format 12 first (full Unicode), then format 4 (BMP).  Returns the
/// first successful encoding record hit.
fn cmap_to_gid_map(cmap_data: &[u8]) -> Result<HashMap<u32, u16>, SubsetError> {
    if cmap_data.len() < 4 {
        return Err(SubsetError::InvalidFont("cmap table too short".into()));
    }
    let num_tables = u16::from_be_bytes([cmap_data[2], cmap_data[3]]) as usize;

    if cmap_data.len() < 4 + num_tables * 8 {
        return Err(SubsetError::InvalidFont(
            "cmap table directory truncated".into(),
        ));
    }

    // Collect all encoding records.
    struct EncodingRecord {
        platform_id: u16,
        encoding_id: u16,
        offset: usize,
    }

    let mut records: Vec<EncodingRecord> = Vec::with_capacity(num_tables);
    for i in 0..num_tables {
        let base = 4 + i * 8;
        let platform_id = u16::from_be_bytes([cmap_data[base], cmap_data[base + 1]]);
        let encoding_id = u16::from_be_bytes([cmap_data[base + 2], cmap_data[base + 3]]);
        let offset = u32::from_be_bytes([
            cmap_data[base + 4],
            cmap_data[base + 5],
            cmap_data[base + 6],
            cmap_data[base + 7],
        ]) as usize;
        records.push(EncodingRecord {
            platform_id,
            encoding_id,
            offset,
        });
    }

    // Prefer format 12 (full Unicode), then format 4 (BMP).
    // Priority: Platform 0 enc 4 / Platform 3 enc 10 (format 12)
    //           Platform 0 enc 3 / Platform 3 enc 1  (format 4)

    let mut result: HashMap<u32, u16> = HashMap::new();
    let mut found_f12 = false;
    let mut found_f4 = false;

    for r in &records {
        if r.offset + 2 > cmap_data.len() {
            continue;
        }
        let format = u16::from_be_bytes([cmap_data[r.offset], cmap_data[r.offset + 1]]);

        match (r.platform_id, r.encoding_id, format) {
            // Format 12 — full Unicode.
            (0, 4, 12) | (3, 10, 12) if !found_f12 => {
                if let Ok(map) = parse_format12(&cmap_data[r.offset..]) {
                    result.extend(map);
                    found_f12 = true;
                }
            }
            // Format 4 — BMP.
            (0, 3, 4) | (3, 1, 4) if !found_f4 => {
                if let Ok(map) = parse_format4(&cmap_data[r.offset..]) {
                    for (cp, gid) in map {
                        result.entry(cp as u32).or_insert(gid);
                    }
                    found_f4 = true;
                }
            }
            _ => {}
        }
    }

    if result.is_empty() {
        // Fall back: try any format 4 record.
        for r in &records {
            if r.offset + 2 > cmap_data.len() {
                continue;
            }
            let format = u16::from_be_bytes([cmap_data[r.offset], cmap_data[r.offset + 1]]);
            if format == 4 {
                if let Ok(map) = parse_format4(&cmap_data[r.offset..]) {
                    for (cp, gid) in map {
                        result.insert(cp as u32, gid);
                    }
                    break;
                }
            }
        }
    }

    Ok(result)
}

fn parse_format4(data: &[u8]) -> Result<Vec<(u16, u16)>, SubsetError> {
    if data.len() < 14 {
        return Err(SubsetError::InvalidFont(
            "format 4 sub-table too short".into(),
        ));
    }
    let seg_count_x2 = u16::from_be_bytes([data[6], data[7]]) as usize;
    let seg_count = seg_count_x2 / 2;

    if seg_count == 0 {
        return Ok(vec![]);
    }

    // Arrays start at offset 14:
    // endCode[0..seg_count]  at 14
    // reservedPad            at 14 + seg_count*2
    // startCode[0..seg_count] at 14 + seg_count*2 + 2
    // idDelta[0..seg_count]   at 14 + seg_count*4 + 2
    // idRangeOffset[0..seg_count] at 14 + seg_count*6 + 2
    // glyphIdArray starts at 14 + seg_count*8 + 2

    let end_code_base = 14usize;
    let start_code_base = end_code_base + seg_count * 2 + 2;
    let id_delta_base = start_code_base + seg_count * 2;
    let id_range_offset_base = id_delta_base + seg_count * 2;
    let glyph_id_array_base = id_range_offset_base + seg_count * 2;

    if data.len() < glyph_id_array_base {
        return Err(SubsetError::InvalidFont(
            "format 4 sub-table truncated".into(),
        ));
    }

    let mut pairs: Vec<(u16, u16)> = Vec::new();

    for i in 0..seg_count {
        let end_code =
            u16::from_be_bytes([data[end_code_base + i * 2], data[end_code_base + i * 2 + 1]]);
        if end_code == 0xFFFF {
            break; // sentinel
        }
        let start_code = u16::from_be_bytes([
            data[start_code_base + i * 2],
            data[start_code_base + i * 2 + 1],
        ]);
        let id_delta =
            i16::from_be_bytes([data[id_delta_base + i * 2], data[id_delta_base + i * 2 + 1]])
                as i32;
        let id_range_offset = u16::from_be_bytes([
            data[id_range_offset_base + i * 2],
            data[id_range_offset_base + i * 2 + 1],
        ]) as usize;

        for cp in start_code..=end_code {
            let gid = if id_range_offset == 0 {
                let raw = (cp as i32 + id_delta) & 0xFFFF;
                raw as u16
            } else {
                // glyphIdArray index.
                let range_ptr_offset = id_range_offset_base + i * 2;
                let idx = range_ptr_offset + id_range_offset + (cp - start_code) as usize * 2;
                if idx + 2 > data.len() {
                    0
                } else {
                    let raw_gid = u16::from_be_bytes([data[idx], data[idx + 1]]);
                    if raw_gid == 0 {
                        0
                    } else {
                        let shifted = (raw_gid as i32 + id_delta) & 0xFFFF;
                        shifted as u16
                    }
                }
            };
            if gid != 0 {
                pairs.push((cp, gid));
            }
        }
    }

    Ok(pairs)
}

fn parse_format12(data: &[u8]) -> Result<HashMap<u32, u16>, SubsetError> {
    if data.len() < 16 {
        return Err(SubsetError::InvalidFont(
            "format 12 sub-table too short".into(),
        ));
    }
    let num_groups = u32::from_be_bytes([data[12], data[13], data[14], data[15]]) as usize;
    if data.len() < 16 + num_groups * 12 {
        return Err(SubsetError::InvalidFont(
            "format 12 sub-table truncated".into(),
        ));
    }
    let mut map = HashMap::with_capacity(num_groups * 4);
    for i in 0..num_groups {
        let base = 16 + i * 12;
        let start_char =
            u32::from_be_bytes([data[base], data[base + 1], data[base + 2], data[base + 3]]);
        let end_char = u32::from_be_bytes([
            data[base + 4],
            data[base + 5],
            data[base + 6],
            data[base + 7],
        ]);
        let start_glyph = u32::from_be_bytes([
            data[base + 8],
            data[base + 9],
            data[base + 10],
            data[base + 11],
        ]);
        let count = end_char.saturating_sub(start_char) + 1;
        for j in 0..count {
            let cp = start_char + j;
            let gid = (start_glyph + j) as u16;
            map.insert(cp, gid);
        }
    }
    Ok(map)
}

// ---------------------------------------------------------------------------
// Composite component closure
// ---------------------------------------------------------------------------

fn expand_gid_set_with_composites(
    glyf_data: &[u8],
    old_loca: &[u8],
    loca_format: i16,
    gids: &mut BTreeSet<u16>,
) {
    // BFS expansion.
    let mut queue: Vec<u16> = gids.iter().copied().collect();
    while let Some(gid) = queue.pop() {
        let (start, end) = match loca_entry_local(old_loca, loca_format, gid) {
            Some(se) => se,
            None => continue,
        };
        if start >= end || end > glyf_data.len() {
            continue;
        }
        let glyph = &glyf_data[start..end];
        if glyph.len() < 2 {
            continue;
        }
        let num_contours = i16::from_be_bytes([glyph[0], glyph[1]]);
        if num_contours < 0 {
            let mut components = Vec::new();
            if glyf::collect_composite_components(glyf_data, start, end, &mut components).is_ok() {
                for comp_gid in components {
                    if gids.insert(comp_gid) {
                        queue.push(comp_gid);
                    }
                }
            }
        }
    }
}

fn loca_entry_local(loca: &[u8], format: i16, gid: u16) -> Option<(usize, usize)> {
    let idx = gid as usize;
    if format == 0 {
        let start_bytes = loca.get(idx * 2..idx * 2 + 2)?;
        let end_bytes = loca.get((idx + 1) * 2..(idx + 1) * 2 + 2)?;
        let start = (u16::from_be_bytes([start_bytes[0], start_bytes[1]]) as usize) * 2;
        let end = (u16::from_be_bytes([end_bytes[0], end_bytes[1]]) as usize) * 2;
        Some((start, end))
    } else {
        let start_bytes = loca.get(idx * 4..idx * 4 + 4)?;
        let end_bytes = loca.get((idx + 1) * 4..(idx + 1) * 4 + 4)?;
        let start = u32::from_be_bytes([
            start_bytes[0],
            start_bytes[1],
            start_bytes[2],
            start_bytes[3],
        ]) as usize;
        let end =
            u32::from_be_bytes([end_bytes[0], end_bytes[1], end_bytes[2], end_bytes[3]]) as usize;
        Some((start, end))
    }
}

// ---------------------------------------------------------------------------
// hmtx / vmtx rewriters
// ---------------------------------------------------------------------------

fn rewrite_metrics_table(
    data: &[u8],
    original_num_metrics: usize,
    original_total_glyphs: usize,
    rev_remap: &HashMap<u16, u16>,
    new_glyph_count: u16,
) -> Vec<u8> {
    // hmtx layout: numLongHorMetrics × (advanceWidth u16, lsb i16)
    // followed by leftSideBearings (i16) for remaining glyphs.
    let mut out = Vec::with_capacity(new_glyph_count as usize * 4);

    let get_metrics = |old_gid: usize| -> (u16, i16) {
        if old_gid < original_num_metrics {
            let base = old_gid * 4;
            if base + 4 <= data.len() {
                let advance = u16::from_be_bytes([data[base], data[base + 1]]);
                let lsb = i16::from_be_bytes([data[base + 2], data[base + 3]]);
                (advance, lsb)
            } else {
                (0, 0)
            }
        } else {
            // Only lsb stored; advance = last long metric's advance.
            let last_advance =
                if original_num_metrics > 0 && (original_num_metrics - 1) * 4 + 1 < data.len() {
                    let base = (original_num_metrics - 1) * 4;
                    u16::from_be_bytes([data[base], data[base + 1]])
                } else {
                    0
                };
            let lsb_idx = old_gid - original_num_metrics;
            let lsb_base = original_num_metrics * 4 + lsb_idx * 2;
            let lsb = if lsb_base + 2 <= data.len() {
                i16::from_be_bytes([data[lsb_base], data[lsb_base + 1]])
            } else {
                0
            };
            (last_advance, lsb)
        }
    };

    for new_gid in 0..new_glyph_count {
        let old_gid = rev_remap.get(&new_gid).copied().unwrap_or(0) as usize;
        let clamped_old = old_gid.min(original_total_glyphs.saturating_sub(1));
        let (advance, lsb) = get_metrics(clamped_old);
        out.extend_from_slice(&advance.to_be_bytes());
        out.extend_from_slice(&lsb.to_be_bytes());
    }

    out
}

// ---------------------------------------------------------------------------
// post v3.0
// ---------------------------------------------------------------------------

fn build_post_v3() -> Vec<u8> {
    // version 3.0 = 0x00030000
    let mut out = vec![0u8; 32];
    out[0] = 0x00;
    out[1] = 0x03;
    out[2] = 0x00;
    out[3] = 0x00;
    // All other fields zero (italic angle, underline pos, underline thickness,
    // isFixedPitch, minMemType42, maxMemType42, minMemType1, maxMemType1).
    out
}

// ---------------------------------------------------------------------------
// name table filter
// ---------------------------------------------------------------------------

/// Retain only name IDs 0–6 from the name table.
fn rewrite_name(name_data: &[u8]) -> Vec<u8> {
    if name_data.len() < 6 {
        return name_data.to_vec();
    }
    let format = u16::from_be_bytes([name_data[0], name_data[1]]);
    let count = u16::from_be_bytes([name_data[2], name_data[3]]) as usize;
    let string_offset = u16::from_be_bytes([name_data[4], name_data[5]]) as usize;

    if format != 0 || name_data.len() < 6 + count * 12 {
        // Non-trivial format — return verbatim.
        return name_data.to_vec();
    }

    // Collect records with nameID 0–6.
    struct NameRecord {
        platform_id: u16,
        encoding_id: u16,
        language_id: u16,
        name_id: u16,
        length: u16,
        str_offset: u16,
    }

    let mut kept: Vec<NameRecord> = Vec::new();
    for i in 0..count {
        let base = 6 + i * 12;
        let name_id = u16::from_be_bytes([name_data[base + 6], name_data[base + 7]]);
        if name_id <= 6 {
            kept.push(NameRecord {
                platform_id: u16::from_be_bytes([name_data[base], name_data[base + 1]]),
                encoding_id: u16::from_be_bytes([name_data[base + 2], name_data[base + 3]]),
                language_id: u16::from_be_bytes([name_data[base + 4], name_data[base + 5]]),
                name_id,
                length: u16::from_be_bytes([name_data[base + 8], name_data[base + 9]]),
                str_offset: u16::from_be_bytes([name_data[base + 10], name_data[base + 11]]),
            });
        }
    }

    let new_count = kept.len() as u16;
    let new_string_offset = (6 + new_count as usize * 12) as u16;

    // Collect string data (deduplicated by new sequential offset).
    let mut string_data: Vec<u8> = Vec::new();
    let mut new_offsets: Vec<u16> = Vec::new();
    for rec in &kept {
        let src_start = string_offset + rec.str_offset as usize;
        let src_end = src_start + rec.length as usize;
        let slice = if src_end <= name_data.len() {
            &name_data[src_start..src_end]
        } else {
            &[]
        };
        new_offsets.push(string_data.len() as u16);
        string_data.extend_from_slice(slice);
    }

    let mut out = Vec::with_capacity(6 + kept.len() * 12 + string_data.len());
    out.extend_from_slice(&0u16.to_be_bytes()); // format 0
    out.extend_from_slice(&new_count.to_be_bytes());
    out.extend_from_slice(&new_string_offset.to_be_bytes());

    for (rec, &new_off) in kept.iter().zip(new_offsets.iter()) {
        out.extend_from_slice(&rec.platform_id.to_be_bytes());
        out.extend_from_slice(&rec.encoding_id.to_be_bytes());
        out.extend_from_slice(&rec.language_id.to_be_bytes());
        out.extend_from_slice(&rec.name_id.to_be_bytes());
        out.extend_from_slice(&rec.length.to_be_bytes());
        out.extend_from_slice(&new_off.to_be_bytes());
    }

    out.extend_from_slice(&string_data);
    out
}

// ---------------------------------------------------------------------------
// Main public functions
// ---------------------------------------------------------------------------

/// Core subsetting engine, operating on an already-parsed table directory.
///
/// `original_size` is what [`SubsetStats::original_size`] should report — the
/// size of the whole input buffer, which for a `ttcf` collection is the whole
/// collection rather than the selected face.
///
/// Returns the subset bytes, the statistics, and the old ↔ new glyph-ID map;
/// the public wrappers drop whichever parts their signature does not carry.
fn subset_from_tables<'a>(
    orig_tables: &HashMap<[u8; 4], &'a [u8]>,
    original_size: usize,
    old_gid_set: &BTreeSet<u16>,
    cp_to_old_gid: &BTreeMap<u32, u16>,
    opts: &SubsetOptions,
) -> Result<(Vec<u8>, SubsetStats, SubsetGidMap), SubsetError> {
    let get_table = |tag: &[u8; 4]| -> Result<&'a [u8], SubsetError> {
        orig_tables
            .get(tag)
            .copied()
            .ok_or(SubsetError::TableMissing(*tag))
    };

    // Detect CFF vs TrueType by presence of CFF  table.
    let is_cff = orig_tables.contains_key(b"CFF ");

    let head_data = get_table(b"head")?;
    let hhea_data = get_table(b"hhea")?;
    let hmtx_data = get_table(b"hmtx")?;
    let maxp_data = get_table(b"maxp")?;

    // -------------------------------------------------------------------------
    // 2. Determine loca format (TrueType only) and do composite expansion.
    // -------------------------------------------------------------------------
    if head_data.len() < 54 {
        return Err(SubsetError::InvalidFont("head table too short".into()));
    }
    let loca_format = get_i16(head_data, 50)
        .ok_or_else(|| SubsetError::InvalidFont("head.indexToLocFormat missing".into()))?;

    // -------------------------------------------------------------------------
    // 3. Expand for composite component closure (TrueType only).
    // -------------------------------------------------------------------------
    // `.notdef` is retained by every entry point's documented contract, and a
    // PDF CIDFont that loses it silently renumbers glyph 0 into whatever the
    // caller's lowest requested glyph was.
    let mut expanded_gid_set = old_gid_set.clone();
    expanded_gid_set.insert(0);
    if !is_cff {
        let glyf_data = get_table(b"glyf")?;
        let loca_data = get_table(b"loca")?;
        expand_gid_set_with_composites(glyf_data, loca_data, loca_format, &mut expanded_gid_set);
    }

    // -------------------------------------------------------------------------
    // 4. Build gid_remap: old GID → new GID (dense, starting at 0).
    // -------------------------------------------------------------------------
    let mut gid_remap: HashMap<u16, u16> = HashMap::with_capacity(expanded_gid_set.len());
    let mut next_new_gid: u16 = 0;
    for &old in &expanded_gid_set {
        gid_remap.insert(old, next_new_gid);
        next_new_gid += 1;
    }
    let new_glyph_count = next_new_gid;

    // Build reverse remap for later.
    let mut rev_remap: HashMap<u16, u16> = HashMap::with_capacity(gid_remap.len());
    for (&old, &new) in &gid_remap {
        rev_remap.insert(new, old);
    }

    // The same renumbering in the form callers can consult: a PDF CIDFont has
    // to emit the *new* GIDs as CIDs, and the composite closure above means
    // they are not predictable from `old_gid_set` alone.
    let gid_map = SubsetGidMap::from_sorted_old_gids(expanded_gid_set.iter().copied().collect());

    // -------------------------------------------------------------------------
    // 5. Rewrite tables.
    // -------------------------------------------------------------------------

    // --- glyf + loca (TrueType only) ---
    let glyf_loca_result: Option<(Vec<u8>, Vec<u8>, i16)> = if !is_cff {
        let glyf_data = get_table(b"glyf")?;
        let loca_data = get_table(b"loca")?;
        let result = glyf::rewrite_glyf_loca(
            glyf_data,
            loca_data,
            loca_format,
            &gid_remap,
            new_glyph_count,
        )?;
        Some(result)
    } else {
        None
    };

    // --- cmap ---
    // Map from surviving codepoint (u32) → new GID.
    let mut cp_to_new_gid: BTreeMap<u32, u16> = BTreeMap::new();
    for (&cp_u32, &old_gid) in cp_to_old_gid {
        if let Some(&ng) = gid_remap.get(&old_gid) {
            cp_to_new_gid.insert(cp_u32, ng);
        }
    }
    let new_cmap = cmap::rewrite_cmap(&cp_to_new_gid)?;

    // --- hmtx ---
    let orig_total_glyphs = get_u16(maxp_data, 4).unwrap_or(0) as usize;
    // numberOfHMetrics is the last u16 in hhea (at offset 34).
    let orig_num_long_metrics = if hhea_data.len() >= 36 {
        get_u16(hhea_data, 34).unwrap_or(0) as usize
    } else {
        orig_total_glyphs
    };
    let new_hmtx = rewrite_metrics_table(
        hmtx_data,
        orig_num_long_metrics,
        orig_total_glyphs,
        &rev_remap,
        new_glyph_count,
    );

    // --- vmtx (optional) ---
    let new_vmtx: Option<Vec<u8>> = if let Some(vmtx_data) = orig_tables.get(b"vmtx") {
        let orig_vhea_long = if let Some(vhea_data) = orig_tables.get(b"vhea") {
            if vhea_data.len() >= 36 {
                get_u16(vhea_data, 34).unwrap_or(0) as usize
            } else {
                orig_total_glyphs
            }
        } else {
            orig_total_glyphs
        };
        Some(rewrite_metrics_table(
            vmtx_data,
            orig_vhea_long,
            orig_total_glyphs,
            &rev_remap,
            new_glyph_count,
        ))
    } else {
        None
    };

    // --- maxp ---
    let mut new_maxp = maxp_data.to_vec();
    set_u16(&mut new_maxp, 4, new_glyph_count);

    // --- head ---
    let mut new_head = head_data.to_vec();
    // Zero checkSumAdjustment (will be recalculated by build_sfnt).
    if new_head.len() >= 12 {
        new_head[8] = 0;
        new_head[9] = 0;
        new_head[10] = 0;
        new_head[11] = 0;
    }
    // Update indexToLocFormat (TrueType only; CFF fonts keep original value).
    if let Some((_, _, new_loca_format)) = glyf_loca_result {
        set_i16(&mut new_head, 50, new_loca_format);
    }

    // --- hhea ---
    let mut new_hhea = hhea_data.to_vec();
    // numberOfHMetrics = new_glyph_count.
    if new_hhea.len() >= 36 {
        set_u16(&mut new_hhea, 34, new_glyph_count);
    }

    // --- vhea (optional) ---
    let new_vhea: Option<Vec<u8>> = orig_tables.get(b"vhea").map(|vhea_data| {
        let mut v = vhea_data.to_vec();
        if v.len() >= 36 {
            set_u16(&mut v, 34, new_glyph_count);
        }
        v
    });

    // --- post ---
    let new_post = build_post_v3();

    // --- name ---
    let name_raw = orig_tables.get(b"name").copied().unwrap_or(&[]);
    // When retaining verbatim, borrow the slice directly to avoid a heap copy.
    let new_name: Cow<'_, [u8]> = if opts.retain_names {
        Cow::Borrowed(name_raw)
    } else {
        Cow::Owned(rewrite_name(name_raw))
    };

    // -------------------------------------------------------------------------
    // 6. Assemble new table list.
    // -------------------------------------------------------------------------

    // Pre-allocated capacity: original font size as upper bound to avoid
    // reallocations during SFNT assembly. Actual subset will be smaller due to
    // removed glyphs and tables. The output_tables vec is pre-sized to ~25
    // slots, covering the ~15 mandatory tables plus common optional ones.
    //
    // `Cow::Borrowed` avoids heap copies for verbatim tables (slices tied to
    // the input `font_data`). Rewritten tables use `Cow::Owned`.
    let mut output_tables: Vec<([u8; 4], Cow<'_, [u8]>)> = Vec::with_capacity(25);

    // Count of advanced GSUB/GPOS subtables dropped during layout rewriting
    // (malformed, or no longer able to match under this subset) — surfaced in
    // [`SubsetStats`] so callers can see how much contextual machinery went.
    let mut dropped_context_subtables = 0usize;

    // Set when the CFF rewriter fell back to copying the source charstrings:
    // correct only under the original glyph numbering, which this subset has
    // just changed. Surfaced in [`SubsetStats`] so an embedder can refuse.
    let mut cff_charstrings_verbatim = false;

    // Verbatim pass-through tags (copy if present, subject to options).
    // Tags for hint tables — omitted when strip_hints=true.
    let hint_tags: &[&[u8; 4]] = &[b"fpgm", b"prep", b"cvt "];
    // Tags for layout tables — omitted when retain_layout_tables=false.
    let layout_tags: &[&[u8; 4]] = &[b"GDEF", b"GPOS", b"GSUB"];
    // Tags that make a face variable — omitted when drop_variations=true.
    // `cvar` and `MVAR` are on the list for completeness; neither is copied by
    // this pipeline in the first place.
    let variation_tags: &[&[u8; 4]] = &[
        b"fvar", b"avar", b"gvar", b"cvar", b"HVAR", b"VVAR", b"MVAR", b"STAT",
    ];

    let verbatim_tags: &[&[u8; 4]] = &[
        // GDEF is handled separately below (rewritten, not verbatim).
        // GSUB is handled separately below (GID references rewritten via otl::rewrite_gsub).
        // GPOS is handled separately below (GID references rewritten via otl_gpos::rewrite_gpos).
        // OS/2 is handled separately below (unicode ranges rewritten).
        // kern is handled separately below (pairs pruned and GIDs remapped).
        b"STAT", b"cvt ", b"fpgm", b"prep", b"gasp", b"feat", b"morx",
        // Variable font axis tables (verbatim — gvar is rewritten separately).
        b"fvar", b"avar", // OS-specific.
        // `DSIG` is deliberately absent: a digital signature covers the bytes of
        // the font it was made for, and every table here has just been
        // rewritten, so carrying it over would ship a signature that is invalid
        // by construction (and 8–10 KB of it on a stock UI font).
        b"BASE",
        // Color: COLR is rewritten separately below; CPAL is GID-independent (verbatim).
        b"CPAL",
        // CBDT/CBLC are rewritten separately below (paired bitmap index/data).
        // MATH is rewritten separately below (Coverage remapping).
    ];
    for &tag in verbatim_tags {
        // Apply strip_hints filter.
        if opts.strip_hints && hint_tags.contains(&tag) {
            continue;
        }
        // Apply retain_layout_tables filter.
        if !opts.retain_layout_tables && layout_tags.contains(&tag) {
            continue;
        }
        // Apply drop_variations filter.
        if opts.drop_variations && variation_tags.contains(&tag) {
            continue;
        }
        if let Some(&data) = orig_tables.get(tag) {
            output_tables.push((*tag, Cow::Borrowed(data)));
        }
    }

    // Pre-compute the surviving codepoints for OS/2 rewriting.
    // (Used in both parallel and sequential paths.)
    let surviving_codepoints: BTreeSet<char> = cp_to_old_gid
        .keys()
        .filter_map(|&cp_u32| char::from_u32(cp_u32))
        .collect();

    // -------------------------------------------------------------------------
    // 6a. Run the heavy independent table rewrites.
    //
    // Under the `parallel` feature, these are dispatched to the rayon thread
    // pool so that GDEF, GSUB, GPOS, OS/2, kern, COLR, SVG, sbix, CBDT/CBLC,
    // MATH, CFF, CFF2, HVAR, VVAR, and gvar can be computed concurrently.
    //
    // All inputs (gid_remap, rev_remap, orig_tables, opts) are read-only during
    // this phase, so sharing them across threads is safe.  The results are
    // collected as `Vec<u8>` (owned) and pushed into `output_tables` afterwards.
    //
    // Under the default (no `parallel` feature) the same rewriters are called
    // sequentially — behaviour and output are identical.
    // -------------------------------------------------------------------------

    #[cfg(feature = "parallel")]
    {
        use rayon::prelude::*;

        // Dropped-context accounting for the parallel path. The rewrite is a
        // pure function of (table bytes, gid_remap), so computing the count here
        // (rather than threading it through the task closures) keeps the task
        // machinery unchanged; it only touches the two layout tables.
        if opts.retain_layout_tables {
            if let Some(&data) = orig_tables.get(b"GSUB") {
                dropped_context_subtables += otl::rewrite_gsub_counted(data, &gid_remap).1;
            }
            if let Some(&data) = orig_tables.get(b"GPOS") {
                dropped_context_subtables += otl_gpos::rewrite_gpos_counted(data, &gid_remap).1;
            }
        }

        // Each task returns zero, one, or two tagged table entries.
        // `Err` propagates from fallible rewriters (HVAR, VVAR) so that a
        // malformed table is still surfaced as a `SubsetError`, matching the
        // behaviour of the sequential path.
        enum TableResult {
            None,
            One([u8; 4], Vec<u8>),
            Two([u8; 4], Vec<u8>, [u8; 4], Vec<u8>),
            /// One table plus the CFF verbatim-fallback indicator.
            Flagged([u8; 4], Vec<u8>, bool),
        }

        // Capture references that are shared read-only across all parallel tasks.
        let gid_remap_ref = &gid_remap;
        let rev_remap_ref = &rev_remap;
        let orig_tables_ref = &orig_tables;
        let surviving_codepoints_ref = &surviving_codepoints;
        let retain_layout = opts.retain_layout_tables;
        let drop_variations = opts.drop_variations;

        // Build task list as a vec of boxed closures.  Each closure captures
        // immutable references to the data it needs and returns
        // `Result<TableResult, SubsetError>`.  Fallible rewriters use `?` inside
        // the closure; infallible ones wrap their output in `Ok(TableResult::One(...))`.
        type Task<'a> = Box<dyn Fn() -> Result<TableResult, SubsetError> + Send + Sync + 'a>;

        let tasks: Vec<Task<'_>> = {
            let mut v: Vec<Task<'_>> = Vec::with_capacity(16);

            // GDEF
            if retain_layout {
                if let Some(&d) = orig_tables_ref.get(b"GDEF") {
                    v.push(Box::new(move || {
                        Ok(TableResult::One(
                            *b"GDEF",
                            layout::rewrite_gdef(d, gid_remap_ref),
                        ))
                    }));
                }
            }
            // GSUB
            if retain_layout {
                if let Some(&d) = orig_tables_ref.get(b"GSUB") {
                    v.push(Box::new(move || {
                        Ok(TableResult::One(
                            *b"GSUB",
                            otl::rewrite_gsub(d, gid_remap_ref),
                        ))
                    }));
                }
            }
            // GPOS
            if retain_layout {
                if let Some(&d) = orig_tables_ref.get(b"GPOS") {
                    v.push(Box::new(move || {
                        Ok(TableResult::One(
                            *b"GPOS",
                            otl_gpos::rewrite_gpos(d, gid_remap_ref),
                        ))
                    }));
                }
            }
            // OS/2
            if let Some(&d) = orig_tables_ref.get(b"OS/2") {
                v.push(Box::new(move || {
                    Ok(TableResult::One(
                        *b"OS/2",
                        os2::rewrite_os2(d, surviving_codepoints_ref),
                    ))
                }));
            }
            // kern
            if let Some(&d) = orig_tables_ref.get(b"kern") {
                v.push(Box::new(move || {
                    let rewritten = kern::rewrite_kern(d, gid_remap_ref);
                    if rewritten.is_empty() {
                        Ok(TableResult::None)
                    } else {
                        Ok(TableResult::One(*b"kern", rewritten))
                    }
                }));
            }
            // COLR
            if let Some(&d) = orig_tables_ref.get(b"COLR") {
                v.push(Box::new(move || {
                    Ok(TableResult::One(
                        *b"COLR",
                        colr::rewrite_colr(d, gid_remap_ref),
                    ))
                }));
            }
            // SVG
            if let Some(&d) = orig_tables_ref.get(b"SVG ") {
                v.push(Box::new(move || {
                    Ok(TableResult::One(
                        *b"SVG ",
                        svg::rewrite_svg(d, gid_remap_ref),
                    ))
                }));
            }
            // sbix
            if let Some(&d) = orig_tables_ref.get(b"sbix") {
                let orig_total = orig_total_glyphs as u16;
                v.push(Box::new(move || {
                    Ok(TableResult::One(
                        *b"sbix",
                        sbix::rewrite_sbix(d, rev_remap_ref, orig_total, new_glyph_count),
                    ))
                }));
            }
            // CBDT/CBLC (paired)
            {
                let cblc_opt = orig_tables_ref.get(b"CBLC").copied();
                let cbdt_opt = orig_tables_ref.get(b"CBDT").copied();
                if let (Some(cblc_d), Some(cbdt_d)) = (cblc_opt, cbdt_opt) {
                    v.push(Box::new(move || {
                        let (new_cblc, new_cbdt) =
                            cbdt::rewrite_cbdt_cblc(cblc_d, cbdt_d, gid_remap_ref);
                        Ok(TableResult::Two(*b"CBLC", new_cblc, *b"CBDT", new_cbdt))
                    }));
                }
            }
            // MATH
            if let Some(&d) = orig_tables_ref.get(b"MATH") {
                v.push(Box::new(move || {
                    Ok(TableResult::One(
                        *b"MATH",
                        math::rewrite_math(d, gid_remap_ref),
                    ))
                }));
            }
            // CFF
            if let Some(&d) = orig_tables_ref.get(b"CFF ") {
                v.push(Box::new(move || {
                    let (bytes, verbatim) = cff::rewrite_cff_checked(d, gid_remap_ref);
                    Ok(TableResult::Flagged(*b"CFF ", bytes, verbatim))
                }));
            }
            // CFF2
            if let Some(&d) = orig_tables_ref.get(b"CFF2") {
                v.push(Box::new(move || {
                    let (bytes, verbatim) = cff::rewrite_cff2_checked(d, gid_remap_ref);
                    Ok(TableResult::Flagged(*b"CFF2", bytes, verbatim))
                }));
            }
            if !drop_variations {
                // HVAR — fallible: propagates SubsetError on malformed table data.
                if let Some(&d) = orig_tables_ref.get(b"HVAR") {
                    v.push(Box::new(move || {
                        let out = varfont::rewrite_hvar_vvar(d, gid_remap_ref, new_glyph_count)?;
                        Ok(TableResult::One(*b"HVAR", out))
                    }));
                }
                // VVAR — fallible: propagates SubsetError on malformed table data.
                if let Some(&d) = orig_tables_ref.get(b"VVAR") {
                    v.push(Box::new(move || {
                        let out = varfont::rewrite_hvar_vvar(d, gid_remap_ref, new_glyph_count)?;
                        Ok(TableResult::One(*b"VVAR", out))
                    }));
                }
                // gvar
                if let Some(&d) = orig_tables_ref.get(b"gvar") {
                    v.push(Box::new(move || {
                        Ok(TableResult::One(
                            *b"gvar",
                            gvar::rewrite_gvar(d, rev_remap_ref, new_glyph_count),
                        ))
                    }));
                }
            }

            v
        };

        // Execute all tasks in parallel; collect into Result so that any error
        // from HVAR/VVAR propagates out of subset_with_gid_set immediately.
        let parallel_results: Vec<TableResult> = tasks
            .par_iter()
            .map(|task| task())
            .collect::<Result<Vec<_>, _>>()?;

        // Push parallel results into output_tables (order within the set of
        // parallel tasks is deterministic since par_iter preserves index order).
        for result in parallel_results {
            match result {
                TableResult::None => {}
                TableResult::One(tag, data) => {
                    output_tables.push((tag, Cow::Owned(data)));
                }
                TableResult::Two(tag1, data1, tag2, data2) => {
                    output_tables.push((tag1, Cow::Owned(data1)));
                    output_tables.push((tag2, Cow::Owned(data2)));
                }
                TableResult::Flagged(tag, data, verbatim) => {
                    cff_charstrings_verbatim |= verbatim;
                    output_tables.push((tag, Cow::Owned(data)));
                }
            }
        }
    }

    // Sequential path: used when the `parallel` feature is not enabled.
    #[cfg(not(feature = "parallel"))]
    {
        // GDEF: rewrite GID references when retain_layout_tables is true.
        if opts.retain_layout_tables {
            if let Some(&data) = orig_tables.get(b"GDEF") {
                output_tables.push((*b"GDEF", Cow::Owned(layout::rewrite_gdef(data, &gid_remap))));
            }
        }

        // GSUB: rewrite GID references (SFL chain + subtables) when retain_layout_tables is true.
        if opts.retain_layout_tables {
            if let Some(&data) = orig_tables.get(b"GSUB") {
                let (bytes, dropped) = otl::rewrite_gsub_counted(data, &gid_remap);
                dropped_context_subtables += dropped;
                output_tables.push((*b"GSUB", Cow::Owned(bytes)));
            }
        }

        // GPOS: rewrite GID references (SFL chain + subtables) when retain_layout_tables is true.
        if opts.retain_layout_tables {
            if let Some(&data) = orig_tables.get(b"GPOS") {
                let (bytes, dropped) = otl_gpos::rewrite_gpos_counted(data, &gid_remap);
                dropped_context_subtables += dropped;
                output_tables.push((*b"GPOS", Cow::Owned(bytes)));
            }
        }

        // OS/2: rewrite unicode range bits and first/last char from surviving codepoints.
        if let Some(&data) = orig_tables.get(b"OS/2") {
            output_tables.push((
                *b"OS/2",
                Cow::Owned(os2::rewrite_os2(data, &surviving_codepoints)),
            ));
        }

        // kern: rewrite pair list, pruning removed GIDs and remapping survivors.
        if let Some(&kern_data) = orig_tables.get(b"kern") {
            let rewritten = kern::rewrite_kern(kern_data, &gid_remap);
            if !rewritten.is_empty() {
                output_tables.push((*b"kern", Cow::Owned(rewritten)));
            }
        }

        // COLR: remap base-glyph and layer GIDs; drop records for removed GIDs.
        // COLR v1+ is preserved verbatim by rewrite_colr.
        if let Some(&colr_data) = orig_tables.get(b"COLR") {
            output_tables.push((
                *b"COLR",
                Cow::Owned(colr::rewrite_colr(colr_data, &gid_remap)),
            ));
        }

        // SVG: remove index entries for removed GID ranges.
        if let Some(&svg_data) = orig_tables.get(b"SVG ") {
            output_tables.push((*b"SVG ", Cow::Owned(svg::rewrite_svg(svg_data, &gid_remap))));
        }

        // sbix: rebuild per-glyph bitmap strike arrays for the new GID space.
        if let Some(&sbix_data) = orig_tables.get(b"sbix") {
            output_tables.push((
                *b"sbix",
                Cow::Owned(sbix::rewrite_sbix(
                    sbix_data,
                    &rev_remap,
                    orig_total_glyphs as u16,
                    new_glyph_count,
                )),
            ));
        }

        // CBDT/CBLC: paired color-bitmap data and index tables.
        // Both must be present to produce a valid result; if only one exists,
        // drop both (a bitmap index without data or data without index is unusable).
        {
            let cblc_opt = orig_tables.get(b"CBLC").copied();
            let cbdt_opt = orig_tables.get(b"CBDT").copied();
            if let (Some(cblc_data), Some(cbdt_data)) = (cblc_opt, cbdt_opt) {
                let (new_cblc, new_cbdt) =
                    cbdt::rewrite_cbdt_cblc(cblc_data, cbdt_data, &gid_remap);
                output_tables.push((*b"CBLC", Cow::Owned(new_cblc)));
                output_tables.push((*b"CBDT", Cow::Owned(new_cbdt)));
            }
            // If only one is present: silently omit both.
        }

        // MATH: remap Coverage tables in MathGlyphInfo and MathVariants.
        if let Some(&math_data) = orig_tables.get(b"MATH") {
            output_tables.push((
                *b"MATH",
                Cow::Owned(math::rewrite_math(math_data, &gid_remap)),
            ));
        }

        // CFF : rewrite for subset GID space (or copy verbatim on parse failure).
        if let Some(&cff_data) = orig_tables.get(b"CFF ") {
            let (bytes, verbatim) = cff::rewrite_cff_checked(cff_data, &gid_remap);
            cff_charstrings_verbatim |= verbatim;
            output_tables.push((*b"CFF ", Cow::Owned(bytes)));
        }
        // CFF2: variable OpenType — rewrite CharStrings for subset GID space.
        // Falls back to verbatim copy on parse failure or CID-keyed fonts.
        if let Some(&cff2_data) = orig_tables.get(b"CFF2") {
            let (bytes, verbatim) = cff::rewrite_cff2_checked(cff2_data, &gid_remap);
            cff_charstrings_verbatim |= verbatim;
            output_tables.push((*b"CFF2", Cow::Owned(bytes)));
        }

        if !opts.drop_variations {
            // HVAR / VVAR
            if let Some(hvar_data) = orig_tables.get(b"HVAR") {
                if let Ok(out) = varfont::rewrite_hvar_vvar(hvar_data, &gid_remap, new_glyph_count)
                {
                    output_tables.push((*b"HVAR", Cow::Owned(out)));
                }
            }
            if let Some(vvar_data) = orig_tables.get(b"VVAR") {
                if let Ok(out) = varfont::rewrite_hvar_vvar(vvar_data, &gid_remap, new_glyph_count)
                {
                    output_tables.push((*b"VVAR", Cow::Owned(out)));
                }
            }

            // gvar
            if let Some(&gvar_data) = orig_tables.get(b"gvar") {
                output_tables.push((
                    *b"gvar",
                    Cow::Owned(gvar::rewrite_gvar(gvar_data, &rev_remap, new_glyph_count)),
                ));
            }
        }
    }

    // Rewritten tables.
    // glyf/loca: TrueType only.
    if let Some((new_glyf, new_loca, _)) = glyf_loca_result {
        output_tables.push((*b"glyf", Cow::Owned(new_glyf)));
        output_tables.push((*b"loca", Cow::Owned(new_loca)));
    }
    output_tables.push((*b"cmap", Cow::Owned(new_cmap)));
    output_tables.push((*b"hmtx", Cow::Owned(new_hmtx)));
    output_tables.push((*b"maxp", Cow::Owned(new_maxp)));
    output_tables.push((*b"head", Cow::Owned(new_head)));
    output_tables.push((*b"hhea", Cow::Owned(new_hhea)));
    output_tables.push((*b"post", Cow::Owned(new_post)));
    output_tables.push((*b"name", new_name));

    if let Some(v) = new_vhea {
        output_tables.push((*b"vhea", Cow::Owned(v)));
    }
    if let Some(v) = new_vmtx {
        output_tables.push((*b"vmtx", Cow::Owned(v)));
    }

    // -------------------------------------------------------------------------
    // 7. Build SFNT.
    // -------------------------------------------------------------------------
    let subset_bytes = tables::build_sfnt(&output_tables);
    let subset_size = subset_bytes.len();

    let tables_retained: Vec<[u8; 4]> = output_tables.iter().map(|(tag, _)| *tag).collect();

    let stats = SubsetStats {
        original_size,
        subset_size,
        glyphs_retained: new_glyph_count,
        tables_retained,
        dropped_context_subtables,
        cff_charstrings_verbatim,
    };

    Ok((subset_bytes, stats, gid_map))
}

/// Core subsetting engine: takes a pre-computed set of old GIDs and an
/// (already-filtered) codepoint→old-GID mapping, applies the full table
/// rewriting pipeline, and returns the subset font bytes together with
/// [`SubsetStats`].
///
/// Callers that need higher-level entry points should use [`subset_font`],
/// [`subset_by_gids`], [`subset_font_for_web`], or [`subset_font_for_pdf`].
/// Use [`subset_with_gid_set_mapped`] to also receive the old ↔ new glyph-ID
/// map, and [`subset_with_gid_set_at_face`] to select a face out of a `ttcf`
/// collection.
///
/// # Errors
/// Returns [`SubsetError`] if the font data is structurally invalid (including
/// a `ttcf` collection, which must go through an `_at_face` entry point) or a
/// required table is absent.
pub fn subset_with_gid_set(
    font_data: &[u8],
    old_gid_set: &BTreeSet<u16>,
    cp_to_old_gid: &BTreeMap<u32, u16>,
    opts: &SubsetOptions,
) -> Result<(Vec<u8>, SubsetStats), SubsetError> {
    let (bytes, stats, _map) =
        subset_with_gid_set_mapped(font_data, old_gid_set, cp_to_old_gid, opts)?;
    Ok((bytes, stats))
}

/// As [`subset_with_gid_set`], additionally returning the [`SubsetGidMap`]
/// describing the old ↔ new glyph-ID renumbering.
///
/// The subset bytes and statistics are identical to [`subset_with_gid_set`];
/// only the map is added.
///
/// # Errors
/// As [`subset_with_gid_set`].
pub fn subset_with_gid_set_mapped(
    font_data: &[u8],
    old_gid_set: &BTreeSet<u16>,
    cp_to_old_gid: &BTreeMap<u32, u16>,
    opts: &SubsetOptions,
) -> Result<(Vec<u8>, SubsetStats, SubsetGidMap), SubsetError> {
    let orig_tables = tables::read_table_directory(font_data)?;
    subset_from_tables(
        &orig_tables,
        font_data.len(),
        old_gid_set,
        cp_to_old_gid,
        opts,
    )
}

/// As [`subset_with_gid_set`], selecting face `face_index` of `font_data`.
///
/// `font_data` may be a plain per-face SFNT (in which case the only valid
/// index is `0` and the behaviour is byte-identical to
/// [`subset_with_gid_set`]) or a `ttcf` collection.
///
/// # Errors
/// Returns [`SubsetError::FaceIndexOutOfRange`] when `face_index` is at or
/// beyond [`face_count`], otherwise as [`subset_with_gid_set`].
pub fn subset_with_gid_set_at_face(
    font_data: &[u8],
    face_index: u32,
    old_gid_set: &BTreeSet<u16>,
    cp_to_old_gid: &BTreeMap<u32, u16>,
    opts: &SubsetOptions,
) -> Result<(Vec<u8>, SubsetStats), SubsetError> {
    let (bytes, stats, _map) = subset_with_gid_set_at_face_mapped(
        font_data,
        face_index,
        old_gid_set,
        cp_to_old_gid,
        opts,
    )?;
    Ok((bytes, stats))
}

/// The fully general byte-slice entry point: face selection *and* the
/// glyph-ID map.
///
/// Equivalent to [`subset_with_gid_set`] with both suffixes applied; every
/// other entry point that takes raw font bytes is a specialisation of it.
/// ([`subset_with_table_map_mapped`] is the equivalent for callers that
/// already hold a parsed `SfntTableMap`.)
///
/// # Errors
/// As [`subset_with_gid_set_at_face`].
pub fn subset_with_gid_set_at_face_mapped(
    font_data: &[u8],
    face_index: u32,
    old_gid_set: &BTreeSet<u16>,
    cp_to_old_gid: &BTreeMap<u32, u16>,
    opts: &SubsetOptions,
) -> Result<(Vec<u8>, SubsetStats, SubsetGidMap), SubsetError> {
    let orig_tables = tables::read_table_directory_at_face(font_data, face_index)?;
    subset_from_tables(
        &orig_tables,
        font_data.len(),
        old_gid_set,
        cp_to_old_gid,
        opts,
    )
}

/// Returns the number of faces `font_data` holds.
///
/// A plain TTF/OTF holds exactly one face; a `ttcf` collection holds its
/// declared `numFonts`. Every index in `0..face_count(data)?` is accepted by
/// the `_at_face` entry points.
///
/// # Errors
/// Returns [`SubsetError::InvalidFont`] when `font_data` is neither a
/// recognised per-face SFNT nor a well-formed `ttcf` collection.
pub fn face_count(font_data: &[u8]) -> Result<u32, SubsetError> {
    oxifont_core::sfnt::face_count(font_data).map_err(tables::map_sfnt_error)
}

/// Subset a TrueType/OpenType font to contain only the given codepoints.
///
/// The output is a minimal valid SFNT containing:
/// - `.notdef` glyph (GID 0, always included).
/// - One glyph per requested codepoint (resolved through cmap).
/// - Composite component glyphs (transitively).
///
/// Tables rewritten: `glyf`, `loca`, `cmap`, `hmtx`, `maxp`, `head`, `hhea`,
/// `post`, `name`.  Variable-font tables (`fvar`, `gvar`, `avar`) are copied
/// verbatim.  `HVAR`/`VVAR` are rewritten if present.  All other tables
/// (GSUB, GPOS, OS/2, kern, …) are copied verbatim.
///
/// This is equivalent to calling [`subset_with_gid_set`] with
/// [`SubsetOptions::default`] after performing the cmap scan.
///
/// # Errors
/// Returns [`SubsetError`] if the font data is structurally invalid or a
/// required table (`glyf`, `loca`, `cmap`, `head`, `hhea`, `hmtx`) is absent.
pub fn subset_font(font_data: &[u8], codepoints: &BTreeSet<char>) -> Result<Vec<u8>, SubsetError> {
    let opts = SubsetOptions::default();
    let (bytes, _stats) = subset_font_with_options(font_data, codepoints, &opts)?;
    Ok(bytes)
}

/// As [`subset_font`], selecting face `face_index` of `font_data`.
///
/// This is the entry point for a `ttcf` collection — every stock Windows CJK
/// family (`msgothic.ttc`, `meiryo.ttc`, `YuGothM.ttc`, `msyh.ttc`, …) ships
/// as one. For a plain TTF/OTF the only valid index is `0`, and the output is
/// byte-identical to [`subset_font`]. Use [`face_count`] to discover the range
/// of valid indices.
///
/// # Errors
/// Returns [`SubsetError::FaceIndexOutOfRange`] when `face_index` is at or
/// beyond [`face_count`], otherwise as [`subset_font`].
pub fn subset_font_at_face(
    font_data: &[u8],
    face_index: u32,
    codepoints: &BTreeSet<char>,
) -> Result<Vec<u8>, SubsetError> {
    let opts = SubsetOptions::default();
    let (bytes, _stats) =
        subset_font_with_options_at_face(font_data, face_index, codepoints, &opts)?;
    Ok(bytes)
}

/// Resolve `codepoints` through the font's `cmap`, honouring
/// [`SubsetOptions::retain_codepoint_range`].
///
/// Returns the old-GID set (always containing `.notdef`) and the surviving
/// codepoint→old-GID map.
fn resolve_codepoints_to_gids(
    orig_tables: &HashMap<[u8; 4], &[u8]>,
    codepoints: &BTreeSet<char>,
    opts: &SubsetOptions,
) -> Result<(BTreeSet<u16>, BTreeMap<u32, u16>), SubsetError> {
    let cmap_data = orig_tables
        .get(b"cmap")
        .copied()
        .ok_or(SubsetError::TableMissing(*b"cmap"))?;

    let cp_to_old_gid_map = cmap_to_gid_map(cmap_data)?;

    let mut old_gid_set: BTreeSet<u16> = BTreeSet::new();
    old_gid_set.insert(0); // always include .notdef

    let mut cp_to_old_gid: BTreeMap<u32, u16> = BTreeMap::new();
    for &cp in codepoints {
        // Apply retain_codepoint_range filter if set.
        if let Some((lo, hi)) = opts.retain_codepoint_range {
            if cp < lo || cp > hi {
                continue;
            }
        }
        let cp_u32 = cp as u32;
        if let Some(&old_gid) = cp_to_old_gid_map.get(&cp_u32) {
            if old_gid != 0 {
                old_gid_set.insert(old_gid);
                cp_to_old_gid.insert(cp_u32, old_gid);
            }
        }
    }

    Ok((old_gid_set, cp_to_old_gid))
}

/// Subset a font with explicit [`SubsetOptions`], returning both the subset
/// bytes and [`SubsetStats`].
///
/// # Errors
/// Returns [`SubsetError`] if the font data is structurally invalid or a
/// required table is absent.
pub fn subset_font_with_options(
    font_data: &[u8],
    codepoints: &BTreeSet<char>,
    opts: &SubsetOptions,
) -> Result<(Vec<u8>, SubsetStats), SubsetError> {
    let (bytes, stats, _map) = subset_font_with_options_mapped(font_data, codepoints, opts)?;
    Ok((bytes, stats))
}

/// As [`subset_font_with_options`], additionally returning the
/// [`SubsetGidMap`] describing the old ↔ new glyph-ID renumbering.
///
/// # Errors
/// As [`subset_font_with_options`].
pub fn subset_font_with_options_mapped(
    font_data: &[u8],
    codepoints: &BTreeSet<char>,
    opts: &SubsetOptions,
) -> Result<(Vec<u8>, SubsetStats, SubsetGidMap), SubsetError> {
    let orig_tables = tables::read_table_directory(font_data)?;
    let (old_gid_set, cp_to_old_gid) = resolve_codepoints_to_gids(&orig_tables, codepoints, opts)?;
    subset_from_tables(
        &orig_tables,
        font_data.len(),
        &old_gid_set,
        &cp_to_old_gid,
        opts,
    )
}

/// As [`subset_font_with_options`], selecting face `face_index` of `font_data`.
///
/// # Errors
/// Returns [`SubsetError::FaceIndexOutOfRange`] when `face_index` is at or
/// beyond [`face_count`], otherwise as [`subset_font_with_options`].
pub fn subset_font_with_options_at_face(
    font_data: &[u8],
    face_index: u32,
    codepoints: &BTreeSet<char>,
    opts: &SubsetOptions,
) -> Result<(Vec<u8>, SubsetStats), SubsetError> {
    let orig_tables = tables::read_table_directory_at_face(font_data, face_index)?;
    let (old_gid_set, cp_to_old_gid) = resolve_codepoints_to_gids(&orig_tables, codepoints, opts)?;
    let (bytes, stats, _map) = subset_from_tables(
        &orig_tables,
        font_data.len(),
        &old_gid_set,
        &cp_to_old_gid,
        opts,
    )?;
    Ok((bytes, stats))
}

/// Subset a font by an explicit set of old GIDs, bypassing the cmap scan.
///
/// The resulting font will have an empty `cmap` table (valid for PDF/print
/// workflows where the cmap is not needed for text extraction by glyph name).
/// Composite component closure is still applied.
///
/// # Errors
/// Returns [`SubsetError`] if the font data is structurally invalid or a
/// required table is absent.
pub fn subset_by_gids(font_data: &[u8], gids: &BTreeSet<u16>) -> Result<Vec<u8>, SubsetError> {
    let (bytes, _stats, _map) = subset_by_gids_mapped(font_data, gids)?;
    Ok(bytes)
}

/// As [`subset_by_gids`], additionally returning the [`SubsetGidMap`].
///
/// This is the direct answer to "which new GID did the glyph I asked for get?"
/// — including for composite components that the closure pulled in but the
/// caller never requested.
///
/// # Errors
/// As [`subset_by_gids`].
pub fn subset_by_gids_mapped(
    font_data: &[u8],
    gids: &BTreeSet<u16>,
) -> Result<(Vec<u8>, SubsetStats, SubsetGidMap), SubsetError> {
    let opts = SubsetOptions::default();
    // Always include .notdef.
    let mut old_gid_set = gids.clone();
    old_gid_set.insert(0);
    // No codepoint mapping — cmap will be empty.
    let cp_to_old_gid: BTreeMap<u32, u16> = BTreeMap::new();
    subset_with_gid_set_mapped(font_data, &old_gid_set, &cp_to_old_gid, &opts)
}

/// As [`subset_by_gids`], selecting face `face_index` of `font_data`.
///
/// # Errors
/// Returns [`SubsetError::FaceIndexOutOfRange`] when `face_index` is at or
/// beyond [`face_count`], otherwise as [`subset_by_gids`].
pub fn subset_by_gids_at_face(
    font_data: &[u8],
    face_index: u32,
    gids: &BTreeSet<u16>,
) -> Result<Vec<u8>, SubsetError> {
    let opts = SubsetOptions::default();
    let mut old_gid_set = gids.clone();
    old_gid_set.insert(0);
    let cp_to_old_gid: BTreeMap<u32, u16> = BTreeMap::new();
    let (bytes, _stats) =
        subset_with_gid_set_at_face(font_data, face_index, &old_gid_set, &cp_to_old_gid, &opts)?;
    Ok(bytes)
}

/// Subset a font for web delivery.
///
/// Preset: `strip_hints = true`, `retain_names = false`.
/// Hint tables (`fpgm`, `prep`, `cvt `) are dropped; only name IDs 0–6 are
/// kept; layout tables are retained.
///
/// # Errors
/// Returns [`SubsetError`] if the font data is structurally invalid or a
/// required table is absent.
pub fn subset_font_for_web(
    font_data: &[u8],
    codepoints: &BTreeSet<char>,
) -> Result<Vec<u8>, SubsetError> {
    let opts = SubsetOptions::default()
        .strip_hints(true)
        .retain_names(false);
    let (bytes, _stats) = subset_font_with_options(font_data, codepoints, &opts)?;
    Ok(bytes)
}

/// Subset a font for PDF embedding.
///
/// Preset: `strip_hints = false`, `retain_names = true`.
/// Hint tables and the full name table are kept; layout tables are retained.
///
/// # Errors
/// Returns [`SubsetError`] if the font data is structurally invalid or a
/// required table is absent.
pub fn subset_font_for_pdf(
    font_data: &[u8],
    codepoints: &BTreeSet<char>,
) -> Result<Vec<u8>, SubsetError> {
    let opts = SubsetOptions::default()
        .strip_hints(false)
        .retain_names(true);
    let (bytes, _stats) = subset_font_with_options(font_data, codepoints, &opts)?;
    Ok(bytes)
}

/// Subset a font using a pre-parsed SFNT table map.
///
/// Equivalent to [`subset_with_gid_set`] but accepts a pre-parsed
/// [`oxifont_core::sfnt::SfntTableMap`] so that callers (e.g. the `oxifont`
/// facade) that have already walked the SFNT directory can skip the second
/// directory parse that would otherwise happen inside the subsetting pipeline.
///
/// `gid_set` is the set of **old** GIDs to retain (`.notdef` = GID 0 is
/// always included implicitly). `cp_to_old_gid` maps Unicode codepoints
/// (as `u32`) to their old GIDs; pass an empty `BTreeMap` when codepoint
/// mapping is not needed (the output `cmap` will be empty, suitable for
/// PDF workflows).
///
/// The map's own tables are used directly, so a map produced by
/// [`SfntTableMap::parse_face`](oxifont_core::sfnt::SfntTableMap::parse_face)
/// or
/// [`parse_at_offset`](oxifont_core::sfnt::SfntTableMap::parse_at_offset)
/// subsets that face of a `ttcf` collection.
///
/// # Errors
/// Returns [`SubsetError`] if the font data is structurally invalid or a
/// required table is absent.
pub fn subset_with_table_map(
    map: &oxifont_core::sfnt::SfntTableMap<'_>,
    gid_set: &BTreeSet<u16>,
    cp_to_old_gid: &BTreeMap<u32, u16>,
    opts: &SubsetOptions,
) -> Result<(Vec<u8>, SubsetStats), SubsetError> {
    let (bytes, stats, _map) = subset_with_table_map_mapped(map, gid_set, cp_to_old_gid, opts)?;
    Ok((bytes, stats))
}

/// As [`subset_with_table_map`], additionally returning the [`SubsetGidMap`].
///
/// # Errors
/// As [`subset_with_table_map`].
pub fn subset_with_table_map_mapped(
    map: &oxifont_core::sfnt::SfntTableMap<'_>,
    gid_set: &BTreeSet<u16>,
    cp_to_old_gid: &BTreeMap<u32, u16>,
    opts: &SubsetOptions,
) -> Result<(Vec<u8>, SubsetStats, SubsetGidMap), SubsetError> {
    // Always include .notdef.
    let mut old_gid_set = gid_set.clone();
    old_gid_set.insert(0);

    let mut orig_tables: HashMap<[u8; 4], &[u8]> = HashMap::with_capacity(map.num_tables());
    for tag in map.tags() {
        if let Some(slice) = map.table(tag) {
            orig_tables.insert(*tag, slice);
        }
    }

    subset_from_tables(
        &orig_tables,
        map.raw().len(),
        &old_gid_set,
        cp_to_old_gid,
        opts,
    )
}
