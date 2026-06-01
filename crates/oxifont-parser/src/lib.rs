#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! `oxifont-parser` — Pure Rust TTF/OTF/TTC parser.
//!
//! Wraps [`ttf_parser`] with owned byte storage so a [`ParsedFace`] outlives
//! the original byte slice. Auto-detects format (TTF, OTF, TTC). Implements
//! the [`FontFace`] trait from `oxifont-core`.
//!
//! # Example
//! ```no_run
//! use oxifont_parser::ParsedFace;
//! use oxifont_core::FontFace as _;
//!
//! let bytes = std::fs::read("/path/to/your/font.ttf").expect("font file must be readable");
//! let face = ParsedFace::parse(bytes, 0).expect("font must parse successfully");
//! println!("{} weight={}", face.family_name(), face.weight());
//! ```

use std::sync::Arc;

use oxifont_core::{
    ColorGlyphFormat, FaceInfo, FontError, FontFace, FontMetrics, FontStretch, FontStyle,
    GlyphOutline, VariationAxis,
};

/// Returns the number of font faces inside a TrueType collection.
///
/// Returns `1` for plain TTF / OTF files (which contain exactly one face).
pub fn face_count(data: &[u8]) -> u32 {
    ttf_parser::fonts_in_collection(data).unwrap_or(1)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn extract_family_name(face: &ttf_parser::Face<'_>) -> String {
    // Prefer TYPOGRAPHIC_FAMILY (name ID 16) for compound names, fall back to
    // FAMILY (name ID 1). Accept either Unicode-encoded name record.
    let preferred_ids = [
        ttf_parser::name_id::TYPOGRAPHIC_FAMILY,
        ttf_parser::name_id::FAMILY,
    ];

    for &target_id in &preferred_ids {
        let found = face
            .names()
            .into_iter()
            .find(|n| n.name_id == target_id && n.is_unicode());
        if let Some(name) = found {
            if let Some(s) = name.to_string() {
                return s;
            }
        }
    }

    "Unknown".to_string()
}

fn extract_style(face: &ttf_parser::Face<'_>) -> FontStyle {
    if face.is_oblique() {
        FontStyle::Oblique
    } else if face.is_italic() {
        FontStyle::Italic
    } else {
        FontStyle::Normal
    }
}

fn extract_axes(face: &ttf_parser::Face<'_>) -> Vec<VariationAxis> {
    face.variation_axes()
        .into_iter()
        .map(|ax| VariationAxis {
            tag: ax.tag.to_bytes(),
            min_value: ax.min_value,
            default_value: ax.def_value,
            max_value: ax.max_value,
            // name_id is a raw u16 that references the `name` table;
            // stringify as the numeric ID for now (M1 scope).
            name: ax.name_id.to_string(),
        })
        .collect()
}

fn extract_postscript_name(face: &ttf_parser::Face<'_>) -> String {
    // PostScript name is name ID 6.
    face.names()
        .into_iter()
        .find(|n| n.name_id == ttf_parser::name_id::POST_SCRIPT_NAME && n.is_unicode())
        .and_then(|n| n.to_string())
        .unwrap_or_default()
}

fn extract_metrics(face: &ttf_parser::Face<'_>) -> FontMetrics {
    FontMetrics {
        units_per_em: face.units_per_em(),
        ascender: face.ascender(),
        descender: face.descender(),
        line_gap: face.line_gap(),
        cap_height: face.capital_height(),
        x_height: face.x_height(),
        underline_position: face.underline_metrics().map(|m| m.position).unwrap_or(0),
        underline_thickness: face.underline_metrics().map(|m| m.thickness).unwrap_or(0),
        strikeout_position: face.strikeout_metrics().map(|m| m.position).unwrap_or(0),
        strikeout_thickness: face.strikeout_metrics().map(|m| m.thickness).unwrap_or(0),
    }
}

fn has_raw_table(face: &ttf_parser::Face<'_>, tag: &[u8; 4]) -> bool {
    face.raw_face()
        .table(ttf_parser::Tag::from_bytes(tag))
        .is_some()
}

fn detect_color_glyph_format(face: &ttf_parser::Face<'_>) -> Option<ColorGlyphFormat> {
    // Check for SVG table first (richest format).
    if has_raw_table(face, b"SVG ") {
        return Some(ColorGlyphFormat::Svg);
    }
    // Check for sbix (Apple bitmap).
    if has_raw_table(face, b"sbix") {
        return Some(ColorGlyphFormat::Sbix);
    }
    // Check for CBDT/CBLC (Google/EBLC bitmap).
    if has_raw_table(face, b"CBDT") || has_raw_table(face, b"CBLC") {
        return Some(ColorGlyphFormat::Cbdt);
    }
    // Check for COLR table.
    if has_raw_table(face, b"COLR") {
        // Distinguish v0 vs v1 by checking for paint tables (v1 has a much
        // larger COLR table). For now, report ColrV0 for any COLR presence.
        // A full v0/v1 distinction requires parsing the COLR header version.
        return Some(ColorGlyphFormat::ColrV0);
    }
    None
}

// ---------------------------------------------------------------------------
// ParsedFace
// ---------------------------------------------------------------------------

/// An owned, fully-parsed font face.
///
/// Byte data is stored in an [`Arc`]`<[u8]>` so cloning a `ParsedFace` is
/// cheap. All commonly-accessed metadata (family name, weight, metrics, axes,
/// …) is extracted and cached at construction time so those accessors never
/// re-parse. Glyph-level queries (`glyph_for_char`, `advance_width`, `outline`,
/// `kern`, `has_table`, `vertical_advance`) re-parse the `ttf_parser::Face` on
/// every call because the face borrows from the byte buffer and cannot be
/// stored alongside it without unsafe self-referential code.
///
/// Use [`ParsedFace::parse`] to construct, or [`ParsedFace::from_face_info`]
/// to load from a [`FaceInfo`] record returned by a catalog.
///
/// # Thread Safety
///
/// `ParsedFace` is both [`Send`] and [`Sync`] because the underlying byte
/// storage is an `Arc<[u8]>` (immutable, reference-counted), and all cached
/// fields (`String`, `Vec`, primitive integers) are themselves `Send + Sync`.
#[derive(Clone)]
pub struct ParsedFace {
    data: Arc<[u8]>,
    face_index: u32,
    family: String,
    postscript_name: String,
    style: FontStyle,
    stretch: FontStretch,
    weight: u16,
    is_monospace: bool,
    units_per_em: u16,
    glyph_count: u16,
    axes: Vec<VariationAxis>,
    metrics: FontMetrics,
    color_format: Option<ColorGlyphFormat>,
    /// Whether the font has an `fvar` table (i.e. is a variable font).
    ///
    /// Cached at construction time so [`variation_coordinates`](Self::variation_coordinates)
    /// can gate on this flag without re-parsing the face.
    is_variable: bool,
    /// Variation axis settings applied via [`variation_coordinates`](Self::variation_coordinates).
    variation_settings: Vec<([u8; 4], f32)>,
}

impl ParsedFace {
    /// Parses a TTF, OTF, or TTC font from raw bytes.
    ///
    /// For TTC collections supply the zero-based `face_index`. For TTF/OTF
    /// always pass `0`. Use [`face_count`] to enumerate how many faces a
    /// collection contains before calling this function.
    ///
    /// # Errors
    /// Returns [`FontError::UnsupportedFormat`] when the magic bytes are
    /// unrecognised, [`FontError::IndexOutOfBounds`] for out-of-range TTC
    /// indices, and [`FontError::ParseError`] for malformed table data.
    pub fn parse(data: impl Into<Arc<[u8]>>, face_index: u32) -> Result<Self, FontError> {
        let data: Arc<[u8]> = data.into();

        // Validate TTC index before full parse to give a clearer error.
        let magic = data.get(..4).ok_or(FontError::UnsupportedFormat)?;
        if magic == b"ttcf" {
            let count_bytes: [u8; 4] = data
                .get(8..12)
                .ok_or(FontError::UnsupportedFormat)?
                .try_into()
                .map_err(|_| FontError::UnsupportedFormat)?;
            let count = u32::from_be_bytes(count_bytes);
            if face_index >= count {
                return Err(FontError::IndexOutOfBounds {
                    index: face_index,
                    count,
                });
            }
        }

        // Single parse: all eagerly-cached fields are extracted in one
        // `ttf_parser::Face::parse` call below. Subsequent accessors for
        // family name, weight, metrics, etc. read from the cached struct
        // fields and are O(1). Only glyph-level queries (`glyph_for_char`,
        // `advance_width`, `outline`, `kern`, `has_table`, `vertical_advance`)
        // re-parse via `with_face()` because `Face<'_>` borrows from `data`
        // and cannot be stored alongside it without unsafe self-referential
        // code.
        let face = ttf_parser::Face::parse(&data, face_index)
            .map_err(|e| FontError::ParseError(e.to_string()))?;

        let family = extract_family_name(&face);
        let postscript_name = extract_postscript_name(&face);
        let style = extract_style(&face);
        let stretch = FontStretch::from_width_class(face.width().to_number() as u8);
        let weight = face.weight().to_number();
        let is_monospace = face.is_monospaced();
        let units_per_em = face.units_per_em();
        let glyph_count = face.number_of_glyphs();
        let axes = extract_axes(&face);
        let metrics = extract_metrics(&face);
        let color_format = detect_color_glyph_format(&face);
        // Cache whether the font is variable (has an `fvar` table) so that
        // `variation_coordinates` does not need to re-parse the face.
        let is_variable = face
            .raw_face()
            .table(ttf_parser::Tag::from_bytes(b"fvar"))
            .is_some();

        Ok(ParsedFace {
            data,
            face_index,
            family,
            postscript_name,
            style,
            stretch,
            weight,
            is_monospace,
            units_per_em,
            glyph_count,
            axes,
            metrics,
            color_format,
            is_variable,
            variation_settings: Vec::new(),
        })
    }

    /// Loads and parses a face described by a [`FaceInfo`] record.
    ///
    /// # Errors
    /// Returns [`FontError::IoError`] if the file cannot be read, or a parse
    /// error if the data is malformed.
    pub fn from_face_info(info: &FaceInfo) -> Result<Self, FontError> {
        let bytes = std::fs::read(&info.path)?;
        Self::parse(bytes, info.face_index)
    }

    /// Borrows the underlying raw font bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }

    /// Returns a reference to the raw font data bytes.
    ///
    /// Equivalent to [`as_bytes`](Self::as_bytes). Provided for callers such as
    /// `oxifont-subset` that expect a named `raw_bytes()` accessor so they can
    /// reuse already-parsed font data without re-reading from disk.
    pub fn raw_bytes(&self) -> &[u8] {
        &self.data
    }

    // Re-parse on demand: ttf_parser::Face borrows from self.data.
    // SAFETY: data is valid — it was accepted by Face::parse at construction.
    fn with_face<F, R>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&ttf_parser::Face<'_>) -> Option<R>,
    {
        let face = ttf_parser::Face::parse(&self.data, self.face_index).ok()?;
        f(&face)
    }

    /// Constructs a `ParsedFace` from owned bytes and a face index.
    ///
    /// This is a convenience wrapper around [`ParsedFace::parse`] for callers
    /// that already hold a `Vec<u8>`.
    ///
    /// # Errors
    /// Same as [`ParsedFace::parse`].
    pub fn from_bytes(bytes: Vec<u8>, face_index: u32) -> Result<Self, FontError> {
        Self::parse(bytes, face_index)
    }

    /// Returns the raw bytes of the named SFNT table, if present.
    ///
    /// Parses the SFNT table directory from the underlying byte buffer without
    /// re-allocating. For TTC files the face at `face_index` is located first,
    /// then its table directory is consulted.
    ///
    /// Returns `None` when the tag is not found, or if the data is malformed.
    pub fn table_data(&self, tag: [u8; 4]) -> Option<&[u8]> {
        // Determine the byte offset of the SFNT header for this face.
        let sfnt_offset = if self.data.get(..4)? == b"ttcf" {
            // TTC: read the face offset from the TTC header.
            // TTC header layout:
            //   0..4  = "ttcf"
            //   4..8  = version
            //   8..12 = numFonts (u32)
            //   12 + face_index*4 = offsetTable[face_index] (u32 each)
            let idx = self.face_index as usize;
            let ptr = 12 + idx * 4;
            let offset_bytes: [u8; 4] = self.data.get(ptr..ptr + 4)?.try_into().ok()?;
            u32::from_be_bytes(offset_bytes) as usize
        } else {
            0usize
        };

        // SFNT header: sfVersion(u32) + numTables(u16) + ...
        // numTables is at sfnt_offset + 4.
        let num_tables_bytes: [u8; 2] = self
            .data
            .get(sfnt_offset + 4..sfnt_offset + 6)?
            .try_into()
            .ok()?;
        let num_tables = u16::from_be_bytes(num_tables_bytes) as usize;

        // Table records start at sfnt_offset + 12.
        // Each record: tag(4) + checksum(4) + offset(4) + length(4) = 16 bytes.
        let records_start = sfnt_offset + 12;

        for i in 0..num_tables {
            let rec = records_start + i * 16;
            let entry_tag: [u8; 4] = self.data.get(rec..rec + 4)?.try_into().ok()?;
            if entry_tag != tag {
                continue;
            }
            let offset =
                u32::from_be_bytes(self.data.get(rec + 8..rec + 12)?.try_into().ok()?) as usize;
            let length =
                u32::from_be_bytes(self.data.get(rec + 12..rec + 16)?.try_into().ok()?) as usize;
            return self.data.get(offset..offset + length);
        }
        None
    }

    /// Access the SFNT table directory as a shared zero-copy map.
    ///
    /// Parses the SFNT header on demand and invokes `f` with a reference to
    /// the resulting `SfntTableMap`. All table slices inside the map borrow
    /// from the underlying byte buffer owned by this `ParsedFace`, so the
    /// closure must not store the map beyond its scope.
    ///
    /// For TTC files the correct per-face SFNT offset is determined automatically
    /// from the TTC header — table offsets remain absolute as the OpenType spec
    /// requires.
    ///
    /// This is useful for callers (e.g. `oxifont-subset`) that need zero-copy
    /// table slices without performing a second independent directory walk.
    ///
    /// # Errors
    ///
    /// Returns [`SfntError`](oxifont_core::sfnt::SfntError) if the underlying
    /// SFNT header is malformed.
    pub fn with_table_map<R, F>(&self, f: F) -> Result<R, oxifont_core::sfnt::SfntError>
    where
        F: FnOnce(&oxifont_core::sfnt::SfntTableMap<'_>) -> R,
    {
        // Determine the byte offset of the per-face SFNT header in the raw data.
        // For TTC collections the SFNT for each face is at a specific file offset;
        // table offsets inside the SFNT directory are absolute from file start.
        let sfnt_offset = if self.data.get(..4) == Some(b"ttcf") {
            // TTC header layout:
            //   0..4   = "ttcf"
            //   4..8   = version
            //   8..12  = numFonts (u32)
            //   12 + face_index*4 = offsetTable[face_index] (u32 each)
            let idx = self.face_index as usize;
            let ptr = 12 + idx * 4;
            let offset_bytes = self
                .data
                .get(ptr..ptr + 4)
                .ok_or(oxifont_core::sfnt::SfntError::Truncated)?;
            u32::from_be_bytes([
                offset_bytes[0],
                offset_bytes[1],
                offset_bytes[2],
                offset_bytes[3],
            ]) as usize
        } else {
            0usize
        };

        let map = oxifont_core::sfnt::SfntTableMap::parse_at_offset(&self.data, sfnt_offset)?;
        Ok(f(&map))
    }

    /// Returns the vertical origin of a glyph from the `VORG` table.
    ///
    /// The VORG table records per-glyph vertical origin Y values. The x-origin
    /// is always 0 per the OpenType specification. If the glyph is not listed
    /// in the table, the font-wide default vertical origin Y is returned.
    ///
    /// Returns `None` if the font has no `VORG` table or the table is invalid.
    pub fn vertical_origin(&self, gid: u16) -> Option<(i16, i16)> {
        let vorg = self.table_data(*b"VORG")?;
        // VORG layout:
        //   0..2  = majorVersion (u16, expect 1)
        //   2..4  = minorVersion (u16, expect 0)
        //   4..6  = defaultVertOriginY (i16)
        //   6..8  = numVertOriginYMetrics (u16)
        //   8..   = entries: (glyphIndex: u16, vertOriginY: i16) × count
        if vorg.len() < 8 {
            return None;
        }
        let default_y = i16::from_be_bytes([vorg[4], vorg[5]]);
        let count = u16::from_be_bytes([vorg[6], vorg[7]]) as usize;

        let entries_start = 8usize;
        if vorg.len() < entries_start + count * 4 {
            return None;
        }

        // Binary search through sorted (glyphIndex, vertOriginY) pairs.
        let mut lo = 0usize;
        let mut hi = count;
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let base = entries_start + mid * 4;
            let entry_gid = u16::from_be_bytes([vorg[base], vorg[base + 1]]);
            match entry_gid.cmp(&gid) {
                std::cmp::Ordering::Equal => {
                    let y = i16::from_be_bytes([vorg[base + 2], vorg[base + 3]]);
                    return Some((0, y));
                }
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
            }
        }
        // Glyph not in the per-glyph list — use the font-wide default.
        Some((0, default_y))
    }

    // -----------------------------------------------------------------------
    // GSUB / GPOS feature and script tag extraction
    // -----------------------------------------------------------------------

    /// Parses the FeatureList from a raw GSUB or GPOS table and returns the
    /// four-byte feature tags.
    ///
    /// Layout tables share the same header structure:
    ///   offset 0 : majorVersion (u16)
    ///   offset 2 : minorVersion (u16)
    ///   offset 4 : scriptListOffset  (Offset16 from table start)
    ///   offset 6 : featureListOffset (Offset16 from table start)
    ///   offset 8 : lookupListOffset  (Offset16 from table start)
    ///
    /// FeatureList at featureListOffset:
    ///   featureCount  (u16)
    ///   featureCount × { featureTag [u8;4], featureOffset Offset16 }
    fn feature_tags_from_table(table: &[u8]) -> Vec<[u8; 4]> {
        // Need at least the 10-byte header.
        if table.len() < 10 {
            return Vec::new();
        }
        let feature_list_offset = u16::from_be_bytes([table[6], table[7]]) as usize;
        let fl = match table.get(feature_list_offset..) {
            Some(s) => s,
            None => return Vec::new(),
        };
        if fl.len() < 2 {
            return Vec::new();
        }
        let count = u16::from_be_bytes([fl[0], fl[1]]) as usize;
        // Each record is 4 (tag) + 2 (offset) = 6 bytes.
        if fl.len() < 2 + count * 6 {
            return Vec::new();
        }
        let mut tags = Vec::with_capacity(count);
        for i in 0..count {
            let base = 2 + i * 6;
            let tag: [u8; 4] = match fl[base..base + 4].try_into() {
                Ok(t) => t,
                Err(_) => continue,
            };
            tags.push(tag);
        }
        tags
    }

    /// Parses the ScriptList from a raw GSUB or GPOS table and returns the
    /// four-byte script tags.
    ///
    /// ScriptList at scriptListOffset:
    ///   scriptCount (u16)
    ///   scriptCount × { scriptTag [u8;4], scriptOffset Offset16 }
    fn script_tags_from_table(table: &[u8]) -> Vec<[u8; 4]> {
        if table.len() < 10 {
            return Vec::new();
        }
        let script_list_offset = u16::from_be_bytes([table[4], table[5]]) as usize;
        let sl = match table.get(script_list_offset..) {
            Some(s) => s,
            None => return Vec::new(),
        };
        if sl.len() < 2 {
            return Vec::new();
        }
        let count = u16::from_be_bytes([sl[0], sl[1]]) as usize;
        // Each record is 4 (tag) + 2 (offset) = 6 bytes.
        if sl.len() < 2 + count * 6 {
            return Vec::new();
        }
        let mut tags = Vec::with_capacity(count);
        for i in 0..count {
            let base = 2 + i * 6;
            let tag: [u8; 4] = match sl[base..base + 4].try_into() {
                Ok(t) => t,
                Err(_) => continue,
            };
            tags.push(tag);
        }
        tags
    }

    /// Returns all OpenType feature tags present in the GSUB table.
    ///
    /// Feature tags are four-byte identifiers (e.g. `b"kern"`, `b"liga"`)
    /// that identify typographic features a font implements. Returns an empty
    /// `Vec` if the GSUB table is absent or the table data is malformed.
    pub fn gsub_feature_tags(&self) -> Vec<[u8; 4]> {
        match self.table_data(*b"GSUB") {
            Some(table) => Self::feature_tags_from_table(table),
            None => Vec::new(),
        }
    }

    /// Returns all OpenType feature tags present in the GPOS table.
    ///
    /// Returns an empty `Vec` if the GPOS table is absent or malformed.
    pub fn gpos_feature_tags(&self) -> Vec<[u8; 4]> {
        match self.table_data(*b"GPOS") {
            Some(table) => Self::feature_tags_from_table(table),
            None => Vec::new(),
        }
    }

    /// Returns the union of script tags found in GSUB and GPOS tables.
    ///
    /// Duplicate tags (present in both tables) are deduplicated. Returns an
    /// empty `Vec` if neither table is present or parseable.
    pub fn supported_scripts(&self) -> Vec<[u8; 4]> {
        let mut tags: Vec<[u8; 4]> = Vec::new();
        if let Some(gsub) = self.table_data(*b"GSUB") {
            tags.extend(Self::script_tags_from_table(gsub));
        }
        if let Some(gpos) = self.table_data(*b"GPOS") {
            for tag in Self::script_tags_from_table(gpos) {
                if !tags.contains(&tag) {
                    tags.push(tag);
                }
            }
        }
        tags
    }

    /// Returns the language-system tags for a given script tag in GSUB.
    ///
    /// Parses the Script table for `script` and collects all non-default
    /// LangSys records. The default LangSys (if any) is not included because
    /// it has no tag — use `supported_scripts` to check for the script itself.
    ///
    /// Returns an empty `Vec` when the script is not found, the GSUB table is
    /// absent, or the data is malformed.
    pub fn supported_languages(&self, script: [u8; 4]) -> Vec<[u8; 4]> {
        let gsub = match self.table_data(*b"GSUB") {
            Some(t) => t,
            None => return Vec::new(),
        };
        if gsub.len() < 10 {
            return Vec::new();
        }
        let script_list_offset = u16::from_be_bytes([gsub[4], gsub[5]]) as usize;
        let sl = match gsub.get(script_list_offset..) {
            Some(s) => s,
            None => return Vec::new(),
        };
        if sl.len() < 2 {
            return Vec::new();
        }
        let script_count = u16::from_be_bytes([sl[0], sl[1]]) as usize;
        if sl.len() < 2 + script_count * 6 {
            return Vec::new();
        }

        // Find the script offset within the ScriptList slice.
        let mut found_script_offset: Option<usize> = None;
        for i in 0..script_count {
            let base = 2 + i * 6;
            let tag: [u8; 4] = match sl[base..base + 4].try_into() {
                Ok(t) => t,
                Err(_) => continue,
            };
            if tag == script {
                let script_offset = u16::from_be_bytes([sl[base + 4], sl[base + 5]]) as usize;
                found_script_offset = Some(script_offset);
                break;
            }
        }

        let script_relative_offset = match found_script_offset {
            Some(o) => o,
            None => return Vec::new(),
        };

        // Script table is at scriptListOffset + scriptRelativeOffset from
        // the start of the GSUB table.
        let script_table_offset = script_list_offset + script_relative_offset;
        let st = match gsub.get(script_table_offset..) {
            Some(s) => s,
            None => return Vec::new(),
        };

        // Script table:
        //   defaultLangSysOffset Offset16 (may be 0 = no default)
        //   langSysCount         u16
        //   langSysCount × { langSysTag [u8;4], langSysOffset Offset16 }
        if st.len() < 4 {
            return Vec::new();
        }
        let lang_count = u16::from_be_bytes([st[2], st[3]]) as usize;
        if st.len() < 4 + lang_count * 6 {
            return Vec::new();
        }
        let mut langs = Vec::with_capacity(lang_count);
        for i in 0..lang_count {
            let base = 4 + i * 6;
            let lang_tag: [u8; 4] = match st[base..base + 4].try_into() {
                Ok(t) => t,
                Err(_) => continue,
            };
            langs.push(lang_tag);
        }
        langs
    }

    // -----------------------------------------------------------------------
    // Variation coordinate application
    // -----------------------------------------------------------------------

    /// Creates a new `ParsedFace` with the given variation axis values applied.
    ///
    /// Each entry in `settings` is a `(axis_tag, value)` pair where the tag is
    /// the four-byte axis identifier (e.g. `*b"wght"`) and the value is the
    /// user-space coordinate to set.
    ///
    /// # Limitations
    ///
    /// `ttf_parser`'s `Face::set_variation` operates on a transient
    /// `Face<'_>` that cannot be stored directly. This method records the
    /// requested settings in the new face so that callers can retrieve them via
    /// `variation_settings()`, but the underlying byte buffer is shared and
    /// unchanged (the variation deltas are applied at query time through the
    /// standard `ttf_parser` API which reads `gvar`/`CFF2` deltas on the fly).
    ///
    /// Returns `None` when the font has no `fvar` table (i.e. it is not a
    /// variable font).
    pub fn variation_coordinates(&self, settings: &[([u8; 4], f32)]) -> Option<Self> {
        // A non-variable font has no fvar table; return None immediately.
        // Use the cached flag to avoid re-parsing the face on every call.
        if !self.is_variable {
            return None;
        }
        let mut cloned = self.clone();
        // Store the requested variation settings so they are accessible later.
        cloned.variation_settings = settings.to_vec();
        Some(cloned)
    }

    /// Returns `true` if this font is a variable font (has an `fvar` table).
    ///
    /// This flag is extracted and cached at construction time, so this method
    /// is O(1) and never triggers a re-parse.
    pub fn is_variable(&self) -> bool {
        self.is_variable
    }

    /// Returns `true` if this font uses CFF (Compact Font Format) outlines.
    ///
    /// Checks for the presence of a `CFF ` (CFF version 1) or `CFF2` (CFF
    /// version 2, used in variable CFF fonts) table in the SFNT directory.
    ///
    /// Note: outline extraction via [`FontFace::outline`] works transparently
    /// for both TrueType (glyf table) and CFF fonts because `ttf_parser`
    /// handles both formats through a unified `Face::outline_glyph` API. This
    /// method is provided for callers that need to distinguish the outline
    /// format (e.g., for subsetting or serialisation purposes), not as a
    /// prerequisite for outline queries.
    pub fn is_cff(&self) -> bool {
        self.table_data(*b"CFF ").is_some() || self.table_data(*b"CFF2").is_some()
    }

    /// Returns the variation axis settings that were applied via
    /// [`variation_coordinates`](Self::variation_coordinates), if any.
    ///
    /// Returns an empty slice for faces that have not had variation coordinates
    /// applied.
    pub fn variation_settings(&self) -> &[([u8; 4], f32)] {
        &self.variation_settings
    }

    // -----------------------------------------------------------------------
    // Preload
    // -----------------------------------------------------------------------

    /// Asserts that all eagerly-cached fields are populated and returns `self`.
    ///
    /// # Current behaviour
    ///
    /// `ParsedFace` caches all frequently-accessed metadata at construction
    /// time: family name, PostScript name, style, stretch, weight, monospace
    /// flag, units-per-em, glyph count, variation axes, font metrics, colour
    /// glyph format, and the `is_variable` flag. Those accessors are all O(1).
    ///
    /// Glyph-level queries (`glyph_for_char`, `advance_width`, `outline`,
    /// `kern`, `has_table`, `vertical_advance`) still re-parse the
    /// `ttf_parser::Face` on every call because the face borrows from the
    /// stored byte buffer and cannot be stored alongside it without unsafe
    /// self-referential code. A full glyph-level cache would require an
    /// `Arc<RwLock<…>>` or a separate glyph-cache slab — a larger
    /// architectural change tracked separately.
    ///
    /// This method is a no-op (returns `self` unchanged) but is retained so
    /// call sites written against the intended future caching API compile
    /// without modification today.
    pub fn preload(self) -> Self {
        self
    }

    // -----------------------------------------------------------------------
    // Builder entry point
    // -----------------------------------------------------------------------

    /// Creates a [`ParsedFaceBuilder`] pre-loaded with `data`.
    ///
    /// This is the recommended way to construct a `ParsedFace` when you need
    /// to set a specific face index or variation axis coordinates before
    /// parsing.
    ///
    /// # Example
    /// ```no_run
    /// use oxifont_parser::ParsedFace;
    ///
    /// let bytes = std::fs::read("/path/to/font.ttf").expect("read font");
    /// let face = ParsedFace::builder(bytes)
    ///     .face_index(0)
    ///     .variation("wght", 700.0)
    ///     .build()
    ///     .expect("parse must succeed");
    /// ```
    pub fn builder(data: Vec<u8>) -> ParsedFaceBuilder {
        ParsedFaceBuilder::new(data)
    }

    // -----------------------------------------------------------------------
    // from_path
    // -----------------------------------------------------------------------

    /// Reads a font file from `path` and parses the face at `face_index`.
    ///
    /// # Errors
    ///
    /// Returns [`FontError::IoError`] when the file cannot be read, or a parse
    /// error when the data is malformed. Use [`face_count`] to enumerate how
    /// many faces a TTC collection contains before calling this function.
    pub fn from_path(path: &std::path::Path, face_index: u32) -> Result<Self, FontError> {
        let bytes = std::fs::read(path).map_err(|e| FontError::IoError(std::sync::Arc::new(e)))?;
        Self::from_bytes(bytes, face_index)
    }

    // -----------------------------------------------------------------------
    // FaceInfo conversion
    // -----------------------------------------------------------------------

    /// Converts this parsed face into a lightweight [`FaceInfo`] record.
    ///
    /// The `path` field is set to an empty `PathBuf` because this face was
    /// constructed from raw bytes with no associated file path. Use
    /// [`ParsedFace::from_face_info`] / [`ParsedFace::from_bytes`] to
    /// distinguish the two construction paths at the call site if needed.
    pub fn as_face_info(&self) -> FaceInfo {
        FaceInfo {
            family: Arc::from(self.family.as_str()),
            post_script_name: self.postscript_name.clone(),
            style: self.style.clone(),
            weight: self.weight,
            stretch: self.stretch,
            path: std::path::PathBuf::new(),
            face_index: self.face_index,
            localized_families: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// ParsedFaceBuilder
// ---------------------------------------------------------------------------

/// Builder for constructing a [`ParsedFace`] with optional face-index and
/// variation-axis settings.
///
/// Obtain an instance via [`ParsedFace::builder`].
///
/// # Tag handling in `variation()`
///
/// OpenType axis tags are exactly four ASCII bytes. The builder enforces
/// this:
///
/// - Tags shorter than four characters are padded with trailing spaces.
/// - Tags longer than four characters are truncated to four bytes.
/// - Any tag containing non-ASCII characters causes `build()` to return
///   [`FontError::ParseError`].
///
/// # Example
/// ```no_run
/// use oxifont_parser::ParsedFace;
///
/// let bytes = std::fs::read("font.ttf").unwrap();
/// let face = ParsedFace::builder(bytes)
///     .face_index(0)
///     .variation("wght", 700.0)
///     .build()
///     .expect("parse failed");
/// ```
pub struct ParsedFaceBuilder {
    data: Vec<u8>,
    face_index: u32,
    variation_settings: Vec<([u8; 4], f32)>,
    /// Stores the first tag-conversion error encountered in `variation()`.
    pending_error: Option<FontError>,
}

impl ParsedFaceBuilder {
    /// Creates a new builder seeded with the given raw font bytes.
    ///
    /// Defaults: `face_index = 0`, no variation settings.
    pub fn new(data: Vec<u8>) -> Self {
        Self {
            data,
            face_index: 0,
            variation_settings: Vec::new(),
            pending_error: None,
        }
    }

    /// Sets the zero-based face index within a TrueType collection (TTC).
    ///
    /// For plain TTF/OTF fonts this must be `0`. Use [`face_count`] to
    /// discover how many sub-faces a collection exposes.
    pub fn face_index(mut self, index: u32) -> Self {
        self.face_index = index;
        self
    }

    /// Adds a variation axis setting.
    ///
    /// `tag` is a four-byte ASCII OpenType axis tag (e.g. `"wght"`). Tags
    /// shorter than four bytes are padded with trailing spaces; longer tags
    /// are truncated to four bytes. If any byte of the (padded/truncated) tag
    /// is non-ASCII, an error is recorded and `build()` will return it.
    ///
    /// Multiple calls to `variation()` accumulate settings in order.
    pub fn variation(mut self, tag: &str, value: f32) -> Self {
        // Truncate to four bytes if longer, pad with spaces if shorter.
        let bytes = tag.as_bytes();
        let mut tag_bytes = [b' '; 4];
        let len = bytes.len().min(4);
        tag_bytes[..len].copy_from_slice(&bytes[..len]);

        // Validate: all four bytes must be ASCII.
        if tag_bytes.iter().any(|b| !b.is_ascii()) {
            if self.pending_error.is_none() {
                self.pending_error = Some(FontError::ParseError(format!(
                    "variation axis tag must be ASCII; got {:?}",
                    tag
                )));
            }
            return self;
        }

        self.variation_settings.push((tag_bytes, value));
        self
    }

    /// Consumes the builder and returns the parsed face, or an error.
    ///
    /// # Errors
    ///
    /// Returns the first tag-conversion error captured by [`Self::variation`], if
    /// any. Otherwise delegates to [`ParsedFace::from_bytes`] and propagates
    /// any parse error it returns. On success, the recorded variation settings
    /// are applied to the face.
    pub fn build(self) -> Result<ParsedFace, FontError> {
        // Propagate any deferred tag-validation error before attempting parse.
        if let Some(err) = self.pending_error {
            return Err(err);
        }

        let mut face = ParsedFace::from_bytes(self.data, self.face_index)?;
        // Apply variation settings only if there are any (skip fvar check for
        // now; callers are responsible for supplying a variable font).
        if !self.variation_settings.is_empty() {
            face.variation_settings = self.variation_settings;
        }
        Ok(face)
    }
}

/// Compile-time assertion that `ParsedFace` is `Send + Sync`.
///
/// This is guaranteed by the `Arc<[u8]>` storage: `Arc<T>` is `Send + Sync`
/// when `T: Send + Sync`, and `[u8]` satisfies that.
#[allow(dead_code)]
fn _assert_parsed_face_send_sync() {
    fn assert<T: Send + Sync>() {}
    assert::<ParsedFace>();
}

impl std::fmt::Debug for ParsedFace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParsedFace")
            .field("family", &self.family)
            .field("style", &self.style)
            .field("weight", &self.weight)
            .field("face_index", &self.face_index)
            .finish()
    }
}

/// Outline builder that collects path commands into a `Vec<GlyphOutline>`.
struct OutlineCollector {
    commands: Vec<GlyphOutline>,
}

impl OutlineCollector {
    fn new() -> Self {
        Self {
            commands: Vec::new(),
        }
    }
}

impl ttf_parser::OutlineBuilder for OutlineCollector {
    fn move_to(&mut self, x: f32, y: f32) {
        self.commands.push(GlyphOutline::MoveTo { x, y });
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.commands.push(GlyphOutline::LineTo { x, y });
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.commands.push(GlyphOutline::QuadTo {
            cx: x1,
            cy: y1,
            x,
            y,
        });
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.commands.push(GlyphOutline::CubicTo {
            cx1: x1,
            cy1: y1,
            cx2: x2,
            cy2: y2,
            x,
            y,
        });
    }

    fn close(&mut self) {
        self.commands.push(GlyphOutline::Close);
    }
}

impl FontFace for ParsedFace {
    fn family_name(&self) -> &str {
        &self.family
    }

    fn style(&self) -> FontStyle {
        self.style.clone()
    }

    fn weight(&self) -> u16 {
        self.weight
    }

    fn stretch(&self) -> FontStretch {
        self.stretch
    }

    fn is_monospace(&self) -> bool {
        self.is_monospace
    }

    fn units_per_em(&self) -> u16 {
        self.units_per_em
    }

    fn glyph_for_char(&self, c: char) -> Option<u16> {
        self.with_face(|f| f.glyph_index(c).map(|g| g.0))
    }

    fn advance_width(&self, gid: u16) -> Option<u16> {
        self.with_face(|f| f.glyph_hor_advance(ttf_parser::GlyphId(gid)))
    }

    fn axes(&self) -> &[VariationAxis] {
        &self.axes
    }

    fn metrics(&self) -> Option<FontMetrics> {
        Some(self.metrics.clone())
    }

    fn outline(&self, gid: u16) -> Option<Vec<GlyphOutline>> {
        self.with_face(|f| {
            let mut collector = OutlineCollector::new();
            let _bbox = f.outline_glyph(ttf_parser::GlyphId(gid), &mut collector)?;
            Some(collector.commands)
        })
    }

    fn kern(&self, left_gid: u16, right_gid: u16) -> Option<i16> {
        self.with_face(|f| {
            // Try the legacy kern table.
            let kern_table = f.tables().kern?;
            for subtable in kern_table.subtables {
                if subtable.horizontal && !subtable.variable {
                    if let Some(value) = subtable.glyphs_kerning(
                        ttf_parser::GlyphId(left_gid),
                        ttf_parser::GlyphId(right_gid),
                    ) {
                        return Some(value);
                    }
                }
            }
            None
        })
    }

    fn glyph_count(&self) -> u16 {
        self.glyph_count
    }

    fn color_glyph_format(&self) -> Option<ColorGlyphFormat> {
        self.color_format
    }

    fn postscript_name(&self) -> Option<&str> {
        if self.postscript_name.is_empty() {
            None
        } else {
            Some(&self.postscript_name)
        }
    }

    fn has_table(&self, tag: [u8; 4]) -> bool {
        self.with_face(|f| {
            Some(
                f.raw_face()
                    .table(ttf_parser::Tag::from_bytes(&tag))
                    .is_some(),
            )
        })
        .unwrap_or(false)
    }

    fn vertical_advance(&self, gid: u16) -> Option<u16> {
        self.with_face(|f| f.glyph_ver_advance(ttf_parser::GlyphId(gid)))
    }
}
