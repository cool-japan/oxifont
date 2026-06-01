//! Font file loading and face metadata extraction.
//!
//! This module reads raw font bytes (TTF / OTF / TTC) and produces
//! [`FaceInfo`] records without retaining the full parse tree in memory.

use crate::face::{FaceInfo, Source};
use oxifont_core::VariationAxis;

/// Name table ID constants (matches OpenType specification).
mod name_id {
    pub const FAMILY: u16 = 1;
    pub const TYPOGRAPHIC_FAMILY: u16 = 16;
    pub const POSTSCRIPT_NAME: u16 = 6;
}

/// Extract the canonical typographic family name from a parsed face.
///
/// Preference order:
/// 1. Name ID 16 (TYPOGRAPHIC_FAMILY), any Unicode record
/// 2. Name ID 1 (FAMILY), any Unicode record
/// 3. Literal `"Unknown"`
fn extract_family_name(face: &ttf_parser::Face<'_>) -> String {
    for &target in &[name_id::TYPOGRAPHIC_FAMILY, name_id::FAMILY] {
        let candidate = face
            .names()
            .into_iter()
            .find(|n| n.name_id == target && n.is_unicode())
            .and_then(|n| n.to_string());
        if let Some(s) = candidate {
            return s;
        }
    }
    "Unknown".to_string()
}

/// Extract the PostScript name (name ID 6) from a parsed face.
fn extract_post_script_name(face: &ttf_parser::Face<'_>) -> String {
    face.names()
        .into_iter()
        .find(|n| n.name_id == name_id::POSTSCRIPT_NAME && n.is_unicode())
        .and_then(|n| n.to_string())
        .unwrap_or_default()
}

/// Build the per-locale family name map by scanning all Name table records
/// with name IDs 1 (FAMILY) or 16 (TYPOGRAPHIC_FAMILY).
///
/// Each entry is `(windows_lcid, family_name_string)`.  Duplicate LCIDs are
/// deduplicated by retaining the first seen value.
fn extract_locale_families(face: &ttf_parser::Face<'_>) -> Vec<(u16, String)> {
    let mut result: Vec<(u16, String)> = Vec::new();
    for record in face.names().into_iter() {
        if record.name_id != name_id::FAMILY && record.name_id != name_id::TYPOGRAPHIC_FAMILY {
            continue;
        }
        if !record.is_unicode() {
            continue;
        }
        let lcid = record.language_id;
        if result.iter().any(|(id, _)| *id == lcid) {
            continue;
        }
        if let Some(s) = record.to_string() {
            result.push((lcid, s));
        }
    }
    result
}

/// Extract variable-font axes from the `fvar` table.
fn extract_axes(face: &ttf_parser::Face<'_>) -> Vec<VariationAxis> {
    face.variation_axes()
        .into_iter()
        .map(|ax| VariationAxis {
            tag: ax.tag.to_bytes(),
            min_value: ax.min_value,
            max_value: ax.max_value,
            default_value: ax.def_value,
            name: String::new(),
        })
        .collect()
}

/// Parse one font face from raw bytes, producing a [`FaceInfo`] record.
///
/// Returns `None` when `ttf_parser::Face::parse` rejects the data (bad magic,
/// index out of range, etc.).
///
/// The returned `FaceInfo` has `id == 0`; the caller (the database) assigns
/// the final unique ID when it inserts the record.
pub fn parse_face_info(data: &[u8], index: u32, source: Source) -> Option<FaceInfo> {
    let face = ttf_parser::Face::parse(data, index).ok()?;

    let family = extract_family_name(&face);
    let post_script_name = extract_post_script_name(&face);
    let locale_families = extract_locale_families(&face);

    // Use the Face's public methods (Weight / Width enums) rather than the
    // internal os2 table, which is not part of the public API surface.
    let weight = face.weight().to_number();
    let italic = face.is_italic() || face.is_oblique();
    // Width::to_number() → 1..=9 matching CSS stretch values directly.
    let stretch = face.width().to_number() as u8;
    let monospaced = face.is_monospaced();
    let variable_axes = extract_axes(&face);

    // Extract the OS/2 unicode range bits as a single u128.
    // UnicodeRanges.0 is already packed: bits 0-31 = ulUnicodeRange1,
    // 32-63 = ulUnicodeRange2, 64-95 = ulUnicodeRange3, 96-127 = ulUnicodeRange4.
    let unicode_ranges = face.unicode_ranges().0;

    Some(FaceInfo {
        id: 0,
        family,
        post_script_name,
        weight,
        italic,
        stretch,
        monospaced,
        source,
        face_index: index,
        variable_axes,
        locale_families,
        unicode_ranges,
    })
}

/// Return the number of faces in a font collection (1 for plain TTF/OTF).
pub fn face_count(data: &[u8]) -> u32 {
    ttf_parser::fonts_in_collection(data).unwrap_or(1)
}
