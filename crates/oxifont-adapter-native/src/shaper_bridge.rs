//! Shaper-integration bridge: native OS font fallback for complex script coverage.
//!
//! This module provides a cross-platform API that shaping engines (such as
//! `oxitext-shape`) can use to obtain raw font bytes for codepoints that their
//! primary font does not cover.
//!
//! The implementation delegates to the best available OS mechanism:
//!
//! | Platform | Mechanism |
//! |----------|-----------|
//! | macOS    | CoreText `CTFontCopyCharacterSet` per-codepoint coverage query (single pass) |
//! | Windows  | `NativeCatalog` (DirectWrite) iteration + `ParsedFace::glyph_for_char` |
//! | other    | `NativeCatalog` (pure filesystem scan) + `ParsedFace::glyph_for_char` |
//!
//! # Design
//!
//! `collect_fallback_fonts_for_text` walks the unique codepoints in `text`,
//! identifies the distinct set of OS fonts needed to cover them all, and returns
//! the raw bytes for each font exactly once (deduplication by file path).
//! Shaping engines can then pass this `Vec<Vec<u8>>` directly to e.g.
//! `SwashShaper::shape_with_fallback`.
//!
//! # Performance
//!
//! On macOS, a **single** CoreText enumeration pass is performed; each descriptor
//! is materialised once and its character set is tested against all missing
//! codepoints simultaneously.  This avoids the N × M overhead of calling
//! `find_font_for_codepoint` once per missing codepoint.
//!
//! On other platforms the catalog is iterated once per unique path (TTC files
//! are only read once regardless of sub-face count).
//!
//! For repeated queries build the fallback set once and cache it at the
//! application layer.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Cross-platform codepoint → font-path resolution (single codepoint)
// ---------------------------------------------------------------------------

/// Resolve a single codepoint to the best OS font that covers it.
///
/// Returns the on-disk path of the first system font whose character set
/// includes `cp`, or `None` when no such font is found.
///
/// Platform dispatch:
/// - macOS: delegates to `coretext::find_font_for_codepoint` which uses
///   CoreText's `CTFontCopyCharacterSet` for O(1) per-font coverage queries.
/// - Windows/other: iterates `NativeCatalog` faces and checks each font file
///   using `ParsedFace::glyph_for_char` via the `oxifont_core::FontFace` trait.
pub fn find_native_font_for_codepoint(cp: char) -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        crate::coretext::find_font_for_codepoint(cp)
    }

    #[cfg(not(target_os = "macos"))]
    {
        find_font_for_codepoint_via_catalog(cp)
    }
}

/// Fallback implementation using `NativeCatalog` + `ParsedFace::glyph_for_char`.
///
/// Used on Windows and Linux where we do not have a per-codepoint OS API.
#[cfg(not(target_os = "macos"))]
fn find_font_for_codepoint_via_catalog(cp: char) -> Option<PathBuf> {
    use oxifont_core::{FontCatalog as _, FontFace as _};
    use oxifont_parser::ParsedFace;

    let catalog = crate::NativeCatalog::system().ok()?;
    let mut checked_paths: HashSet<&std::path::Path> = HashSet::new();

    for face_info in catalog.faces() {
        if !checked_paths.insert(face_info.path.as_path()) {
            continue;
        }
        let bytes = match std::fs::read(&face_info.path) {
            Ok(b) => b,
            Err(_) => continue,
        };
        if let Ok(parsed) = ParsedFace::parse(bytes, 0) {
            if parsed.glyph_for_char(cp).is_some() {
                return Some(face_info.path.clone());
            }
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Batched codepoint → font-path resolution (multiple codepoints, one pass)
// ---------------------------------------------------------------------------

/// Map each codepoint in `codepoints` to the path of the best OS font for it.
///
/// On macOS this performs a **single** CoreText enumeration and tests all
/// codepoints against each font's character set in one pass, achieving
/// O(fonts × codepoints) rather than O(codepoints × fonts).
///
/// On other platforms this iterates the catalog once per file path (all
/// codepoints are tested per font file in a single `ParsedFace` query).
///
/// Returns a `HashMap<char, PathBuf>` mapping each resolved codepoint to a
/// font path; codepoints with no covering font are absent from the map.
fn batch_resolve_codepoints(codepoints: &[char]) -> HashMap<char, PathBuf> {
    if codepoints.is_empty() {
        return HashMap::new();
    }

    #[cfg(target_os = "macos")]
    {
        batch_resolve_codepoints_coretext(codepoints)
    }

    #[cfg(not(target_os = "macos"))]
    {
        batch_resolve_codepoints_catalog(codepoints)
    }
}

/// macOS implementation: single CoreText enumeration pass.
///
/// For each descriptor in the system collection, materialises a temporary
/// CTFont at size 0, copies its character set once, and tests all outstanding
/// (not-yet-resolved) codepoints against it.  Stops early when all codepoints
/// have been assigned.
#[cfg(target_os = "macos")]
fn batch_resolve_codepoints_coretext(codepoints: &[char]) -> HashMap<char, PathBuf> {
    use core_foundation::array::CFArray;
    use core_foundation::base::TCFType;
    use core_foundation::characterset::CFCharacterSet;
    use core_text::font::new_from_descriptor;
    use core_text::font_collection;
    use core_text::font_descriptor::CTFontDescriptor;

    // FFI declaration for CoreText character set query.
    extern "C" {
        fn CTFontCopyCharacterSet(
            font: *const std::ffi::c_void,
        ) -> core_foundation::characterset::CFCharacterSetRef;
    }

    let collection = font_collection::create_for_all_families();
    let descriptors: CFArray<CTFontDescriptor> = match collection.get_descriptors() {
        Some(d) => d,
        None => return HashMap::new(),
    };

    // Remaining codepoints to resolve (removed as we find fonts for them).
    let mut remaining: HashMap<char, ()> = codepoints.iter().map(|&c| (c, ())).collect();
    let mut result: HashMap<char, PathBuf> = HashMap::with_capacity(codepoints.len());

    for descriptor in descriptors.iter() {
        if remaining.is_empty() {
            break; // all codepoints resolved — stop early
        }

        let path = match descriptor.font_path() {
            Some(p) => p,
            None => continue,
        };

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_default();
        if !matches!(ext.as_str(), "ttf" | "otf" | "ttc") {
            continue;
        }

        // Materialise CTFont at size 0 and copy its character set.
        // SAFETY: descriptor is a valid CTFontDescriptor; closure is UnwindSafe.
        let ct_font = match std::panic::catch_unwind(move || new_from_descriptor(&descriptor, 0.0))
        {
            Ok(f) => f,
            Err(_) => continue,
        };

        // SAFETY: ct_font.as_concrete_TypeRef() is a valid non-null CTFontRef.
        let cs_ref = unsafe {
            CTFontCopyCharacterSet(ct_font.as_concrete_TypeRef() as *const std::ffi::c_void)
        };
        if cs_ref.is_null() {
            continue;
        }

        // Transfer ownership so Drop releases the +1 retain.
        // SAFETY: cs_ref is +1 retained from CTFontCopyCharacterSet.
        let char_set = unsafe { CFCharacterSet::wrap_under_create_rule(cs_ref) };

        // Test each remaining codepoint against this font's character set.
        let mut newly_resolved: Vec<char> = Vec::new();
        for &cp in remaining.keys() {
            let scalar = cp as u32;
            // SAFETY: char_set.as_concrete_TypeRef() is a valid CFCharacterSetRef.
            let is_member = unsafe {
                core_foundation::characterset::CFCharacterSetIsLongCharacterMember(
                    char_set.as_concrete_TypeRef(),
                    scalar,
                )
            };
            if is_member != 0 {
                newly_resolved.push(cp);
            }
        }

        for cp in newly_resolved {
            remaining.remove(&cp);
            result.insert(cp, path.clone());
        }
    }

    result
}

/// Non-macOS implementation: single catalog iteration, all codepoints per file.
#[cfg(not(target_os = "macos"))]
fn batch_resolve_codepoints_catalog(codepoints: &[char]) -> HashMap<char, PathBuf> {
    use oxifont_core::{FontCatalog as _, FontFace as _};
    use oxifont_parser::ParsedFace;

    let catalog = match crate::NativeCatalog::system() {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };

    let mut remaining: HashMap<char, ()> = codepoints.iter().map(|&c| (c, ())).collect();
    let mut result: HashMap<char, PathBuf> = HashMap::with_capacity(codepoints.len());
    let mut checked_paths: HashSet<PathBuf> = HashSet::new();

    for face_info in catalog.faces() {
        if remaining.is_empty() {
            break;
        }
        if !checked_paths.insert(face_info.path.clone()) {
            continue;
        }
        let bytes = match std::fs::read(&face_info.path) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let parsed = match ParsedFace::parse(bytes, 0) {
            Ok(f) => f,
            Err(_) => continue,
        };

        let mut newly_resolved: Vec<char> = Vec::new();
        for &cp in remaining.keys() {
            if parsed.glyph_for_char(cp).is_some() {
                newly_resolved.push(cp);
            }
        }
        for cp in newly_resolved {
            remaining.remove(&cp);
            result.insert(cp, face_info.path.clone());
        }
    }

    result
}

// ---------------------------------------------------------------------------
// Public API — collect fallback fonts for a text string
// ---------------------------------------------------------------------------

/// Collect raw font bytes for all OS fonts needed to cover `text`.
///
/// For each unique codepoint in `text` that is not covered by the primary
/// font (identified by `primary_font_data`), this function finds the best OS
/// font and loads its bytes.  Each discovered font appears **at most once** in
/// the result — if the same font covers multiple missing codepoints it is
/// returned as a single entry.
///
/// The returned `Vec<Vec<u8>>` is ordered by first-seen missing codepoint
/// (earliest in `text` first), which is a stable, deterministic order suitable
/// for passing to shaping engines as a fallback chain.
///
/// # Arguments
///
/// * `text` — the UTF-8 string that needs to be shaped.
/// * `primary_font_data` — raw bytes of the primary font.  Any codepoint
///   covered by the primary font is excluded from the fallback search.
///   Pass an empty slice `&[]` to treat all codepoints as uncovered.
///
/// # Return value
///
/// A `Vec<Vec<u8>>` of fallback font files (raw SFNT bytes).  An empty `Vec`
/// means the primary font covers all codepoints, or no suitable fallback was
/// found.
///
/// # Example
///
/// ```no_run
/// use oxifont_adapter_native::shaper_bridge::collect_fallback_fonts_for_text;
///
/// let noto_sans = std::fs::read("NotoSans-Regular.ttf").unwrap();
/// let fallbacks = collect_fallback_fonts_for_text("Hello 中文 مرحبا", &noto_sans);
/// println!("{} fallback font(s) needed", fallbacks.len());
/// ```
pub fn collect_fallback_fonts_for_text(text: &str, primary_font_data: &[u8]) -> Vec<Vec<u8>> {
    if text.is_empty() {
        return Vec::new();
    }

    // Determine which codepoints the primary font covers (only for those in text).
    let covered_by_primary = if primary_font_data.is_empty() {
        HashSet::new()
    } else {
        codepoints_covered_by_primary(primary_font_data, text)
    };

    // Collect unique non-whitespace missing codepoints in first-seen order.
    let mut seen: HashSet<char> = HashSet::new();
    let mut missing: Vec<char> = Vec::new();
    for ch in text.chars() {
        if seen.insert(ch) && !ch.is_whitespace() && !covered_by_primary.contains(&ch) {
            missing.push(ch);
        }
    }

    if missing.is_empty() {
        return Vec::new();
    }

    // Resolve all missing codepoints to font paths in a single enumeration pass.
    let cp_to_path = batch_resolve_codepoints(&missing);

    // Build ordered, deduplicated result.
    path_map_to_bytes_vec(&missing, &cp_to_path)
}

/// Collect raw font bytes for all OS fonts needed to cover `text`,
/// without excluding any codepoints based on a primary font.
///
/// Equivalent to calling [`collect_fallback_fonts_for_text`] with an empty
/// `primary_font_data` slice.  Useful when building a full font stack from
/// scratch.
pub fn collect_fonts_for_text(text: &str) -> Vec<Vec<u8>> {
    if text.is_empty() {
        return Vec::new();
    }

    let mut seen: HashSet<char> = HashSet::new();
    let mut unique: Vec<char> = Vec::new();
    for ch in text.chars() {
        if seen.insert(ch) && !ch.is_whitespace() {
            unique.push(ch);
        }
    }

    if unique.is_empty() {
        return Vec::new();
    }

    let cp_to_path = batch_resolve_codepoints(&unique);
    path_map_to_bytes_vec(&unique, &cp_to_path)
}

/// Load the best single OS font for the given `text`.
///
/// Uses the first non-whitespace codepoint in `text` as the representative
/// codepoint for font selection.
///
/// Returns `None` when the OS font enumeration fails, the text is empty, or
/// the text contains only whitespace.
pub fn load_best_native_font_for_text(text: &str) -> Option<Vec<u8>> {
    let first = text.chars().find(|c| !c.is_whitespace())?;
    let path = find_native_font_for_codepoint(first)?;
    std::fs::read(&path).ok()
}

/// Load the best OS font for a specific codepoint and return both bytes and
/// face index.
///
/// The face index is needed for TTC collections — shaping engines must pass
/// it to `FontRef::from_index` (swash) or `Face::from_slice` (rustybuzz).
///
/// Returns `None` when no OS font covers the codepoint or the file cannot
/// be read.
pub fn load_native_font_for_codepoint_with_index(cp: char) -> Option<(Vec<u8>, u32)> {
    #[cfg(target_os = "macos")]
    {
        crate::coretext::load_fallback_font_bytes_with_index(cp)
    }

    #[cfg(not(target_os = "macos"))]
    {
        let bytes = load_best_native_font_for_text(&cp.to_string())?;
        Some((bytes, 0))
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Return the set of codepoints from `text` that are covered by `font_data`.
///
/// Uses `ParsedFace::glyph_for_char` to test only the codepoints that appear
/// in `text` — O(unique chars in text), not O(all of Unicode).
fn codepoints_covered_by_primary(font_data: &[u8], text: &str) -> HashSet<char> {
    use oxifont_core::FontFace as _;
    use oxifont_parser::ParsedFace;

    let face = match ParsedFace::parse(font_data.to_vec(), 0) {
        Ok(f) => f,
        Err(_) => return HashSet::new(),
    };

    text.chars()
        .filter(|&c| !c.is_whitespace())
        .collect::<HashSet<_>>()
        .into_iter()
        .filter(|&c| face.glyph_for_char(c).is_some())
        .collect()
}

/// Convert a `(codepoint → path)` map into an ordered, deduplicated `Vec<Vec<u8>>`.
///
/// The output order follows the first-occurrence order of codepoints in
/// `ordered_codepoints`.  Each unique path contributes at most one entry.
fn path_map_to_bytes_vec(
    ordered_codepoints: &[char],
    cp_to_path: &HashMap<char, PathBuf>,
) -> Vec<Vec<u8>> {
    let mut path_to_bytes: HashMap<PathBuf, Vec<u8>> = HashMap::new();
    let mut ordered_paths: Vec<PathBuf> = Vec::new();

    for cp in ordered_codepoints {
        if let Some(path) = cp_to_path.get(cp) {
            if !path_to_bytes.contains_key(path) {
                if let Ok(bytes) = std::fs::read(path) {
                    ordered_paths.push(path.clone());
                    path_to_bytes.insert(path.clone(), bytes);
                }
            }
        }
    }

    ordered_paths
        .into_iter()
        .filter_map(|p| path_to_bytes.remove(&p))
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_returns_empty_fallbacks() {
        let result = collect_fallback_fonts_for_text("", &[]);
        assert!(result.is_empty());
    }

    #[test]
    fn empty_text_collect_fonts_empty() {
        assert!(collect_fonts_for_text("").is_empty());
    }

    #[test]
    fn load_best_native_font_for_empty_returns_none() {
        assert!(load_best_native_font_for_text("").is_none());
    }

    #[test]
    fn load_best_native_font_for_whitespace_returns_none() {
        assert!(load_best_native_font_for_text("   \t\n").is_none());
    }

    #[test]
    fn empty_primary_font_does_not_panic() {
        // Just verify no panic; result depends on system fonts.
        let _result = collect_fallback_fonts_for_text("A", &[]);
    }

    #[cfg(target_os = "macos")]
    mod macos {
        use super::*;

        #[test]
        fn find_native_font_for_ascii() {
            let path = find_native_font_for_codepoint('A');
            assert!(
                path.is_some(),
                "find_native_font_for_codepoint('A') must return Some on macOS"
            );
        }

        #[test]
        fn find_native_font_path_exists() {
            if let Some(path) = find_native_font_for_codepoint('A') {
                assert!(path.exists(), "font path must exist: {path:?}");
            }
        }

        #[test]
        fn load_font_with_index_ascii() {
            let result = load_native_font_for_codepoint_with_index('A');
            assert!(result.is_some(), "must return Some for ASCII 'A'");
            let (bytes, _index) = result.expect("already checked Some");
            assert!(bytes.len() > 100, "font bytes must be non-trivially sized");
            let magic_ok = matches!(
                bytes[..4],
                [0x00, 0x01, 0x00, 0x00]
                    | [0x4F, 0x54, 0x54, 0x4F]
                    | [0x74, 0x72, 0x75, 0x65]
                    | [0x74, 0x74, 0x63, 0x66]
            );
            assert!(magic_ok, "font must have valid magic: {:02X?}", &bytes[..4]);
        }

        #[test]
        fn collect_fonts_for_ascii_text() {
            let fonts = collect_fonts_for_text("Hello");
            assert!(
                !fonts.is_empty(),
                "must return at least one font for ASCII on macOS"
            );
            for bytes in &fonts {
                assert!(bytes.len() > 100);
            }
        }

        #[test]
        fn load_best_native_font_for_ascii_text() {
            let result = load_best_native_font_for_text("Hello");
            assert!(result.is_some(), "must return Some for ASCII on macOS");
        }

        #[test]
        fn collect_fonts_deduplicates_by_path() {
            // Single unique codepoint → at most one font entry.
            let fonts = collect_fonts_for_text("AAAA");
            assert!(
                fonts.len() <= 1,
                "identical codepoints must not produce duplicate entries"
            );
        }

        #[test]
        fn collect_fallback_does_not_panic() {
            // Verify no panic; result is system-dependent.
            let _result = collect_fallback_fonts_for_text("Hello World", &[]);
        }

        #[test]
        fn collect_fallback_returns_valid_font_bytes() {
            let fonts = collect_fallback_fonts_for_text("Hello", &[]);
            for bytes in &fonts {
                assert!(bytes.len() > 100, "font must be non-trivially sized");
                let magic_ok = matches!(
                    bytes[..4],
                    [0x00, 0x01, 0x00, 0x00]
                        | [0x4F, 0x54, 0x54, 0x4F]
                        | [0x74, 0x72, 0x75, 0x65]
                        | [0x74, 0x74, 0x63, 0x66]
                );
                assert!(magic_ok, "font bytes must have valid font magic");
            }
        }
    }
}
