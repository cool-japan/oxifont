#![forbid(unsafe_code)]
#![warn(missing_docs)]
// Partial no_std stub: full compliance requires alloc imports for String/Vec/Arc
// and core::fmt + core::error instead of std::*. FaceInfo.path: PathBuf is the
// remaining pinch point — gating it requires a larger refactor (deferred).
#![cfg_attr(not(feature = "std"), no_std)]

//! `oxifont-core` — shared types and traits for the OxiFont ecosystem.
//!
//! This crate is the trait surface and shared error/data types used by every
//! other crate in the `oxifont` family. It has **zero external dependencies**
//! and is `no_std`-compatible when `alloc` is available.
//!
//! See [`FontFace`], [`FontCatalog`], [`FaceInfo`], [`FontQuery`], and
//! [`FontError`] for the primary entry points.

extern crate alloc;

pub mod axis;
pub mod error;
pub mod info;
#[cfg(feature = "std")]
pub mod platform_dirs;
pub mod sfnt;
pub mod traits;
pub mod types;

pub use axis::*;
pub use error::*;
pub use info::*;
pub use traits::*;
pub use types::*;

#[cfg(test)]
mod tests {
    use super::*;

    // --- FontStretch ---

    #[test]
    fn font_stretch_default_is_normal() {
        assert_eq!(FontStretch::default(), FontStretch::Normal);
    }

    #[test]
    fn font_stretch_from_width_class_clamps_zero() {
        assert_eq!(
            FontStretch::from_width_class(0),
            FontStretch::UltraCondensed
        );
    }

    #[test]
    fn font_stretch_from_width_class_clamps_high() {
        assert_eq!(
            FontStretch::from_width_class(255),
            FontStretch::UltraExpanded
        );
    }

    #[test]
    fn font_stretch_round_trip() {
        for val in 1..=9u8 {
            let stretch = FontStretch::from_width_class(val);
            assert_eq!(stretch.to_width_class(), val);
        }
    }

    #[test]
    fn font_stretch_ordering() {
        assert!(FontStretch::UltraCondensed < FontStretch::Normal);
        assert!(FontStretch::Normal < FontStretch::UltraExpanded);
    }

    #[test]
    fn font_stretch_display() {
        assert_eq!(FontStretch::Normal.to_string(), "normal");
        assert_eq!(FontStretch::Condensed.to_string(), "condensed");
        assert_eq!(FontStretch::UltraExpanded.to_string(), "ultra-expanded");
    }

    // --- FontStyle ---

    #[test]
    fn font_style_ordering() {
        assert!(FontStyle::Normal < FontStyle::Italic);
        assert!(FontStyle::Italic < FontStyle::Oblique);
        assert!(FontStyle::Normal < FontStyle::Oblique);
    }

    #[test]
    fn font_style_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(FontStyle::Normal);
        set.insert(FontStyle::Italic);
        set.insert(FontStyle::Oblique);
        set.insert(FontStyle::Normal); // duplicate
        assert_eq!(set.len(), 3);
    }

    // --- EmbeddingLicense ---

    #[test]
    fn embedding_license_installable() {
        assert_eq!(
            EmbeddingLicense::from_fs_type(0),
            EmbeddingLicense::Installable
        );
    }

    #[test]
    fn embedding_license_restricted() {
        assert_eq!(
            EmbeddingLicense::from_fs_type(0x0002),
            EmbeddingLicense::Restricted
        );
    }

    #[test]
    fn embedding_license_print_and_preview() {
        assert_eq!(
            EmbeddingLicense::from_fs_type(0x0004),
            EmbeddingLicense::PrintAndPreview
        );
    }

    #[test]
    fn embedding_license_editable() {
        assert_eq!(
            EmbeddingLicense::from_fs_type(0x0008),
            EmbeddingLicense::Editable
        );
    }

    #[test]
    fn embedding_license_no_subsetting() {
        assert_eq!(
            EmbeddingLicense::from_fs_type(0x0100),
            EmbeddingLicense::NoSubsetting
        );
        // Combined with editable class -- modifier wins
        assert_eq!(
            EmbeddingLicense::from_fs_type(0x0108),
            EmbeddingLicense::NoSubsetting
        );
    }

    #[test]
    fn embedding_license_bitmap_only() {
        assert_eq!(
            EmbeddingLicense::from_fs_type(0x0200),
            EmbeddingLicense::BitmapOnly
        );
        // BitmapOnly beats NoSubsetting when both set
        assert_eq!(
            EmbeddingLicense::from_fs_type(0x0300),
            EmbeddingLicense::BitmapOnly
        );
    }

    /// Verify that `from_fs_type(0)` yields `Installable`, meaning the font has
    /// no embedding restrictions — the "no bits set" case is the most permissive
    /// embedding class per the OpenType spec.
    #[test]
    fn test_embedding_license_from_fs_type_zero() {
        let license = EmbeddingLicense::from_fs_type(0);
        assert_eq!(license, EmbeddingLicense::Installable);
        // Confirm it is not any of the restricted variants
        assert_ne!(license, EmbeddingLicense::Restricted);
        assert_ne!(license, EmbeddingLicense::PrintAndPreview);
        assert_ne!(license, EmbeddingLicense::Editable);
        assert_ne!(license, EmbeddingLicense::NoSubsetting);
        assert_ne!(license, EmbeddingLicense::BitmapOnly);
    }

    // --- FontQuery ---

    #[test]
    fn font_query_stretch_builder() {
        let q = FontQuery::new().stretch(FontStretch::Condensed);
        assert_eq!(q.stretch, Some(FontStretch::Condensed));
    }

    #[test]
    fn font_query_all_fields() {
        let q = FontQuery::new()
            .family("Arial")
            .style(FontStyle::Italic)
            .weight(700)
            .stretch(FontStretch::SemiCondensed);
        assert_eq!(q.family.as_deref(), Some("Arial"));
        assert_eq!(q.style, Some(FontStyle::Italic));
        assert_eq!(q.weight, Some(700));
        assert_eq!(q.stretch, Some(FontStretch::SemiCondensed));
    }

    #[test]
    fn font_query_postscript_name_builder() {
        let q = FontQuery::new().postscript_name("Arial-BoldMT");
        assert_eq!(q.postscript_name.as_deref(), Some("Arial-BoldMT"));
    }

    #[test]
    fn font_query_hash_distinct() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(FontQuery::new().family("Arial"));
        set.insert(FontQuery::new().family("Helvetica"));
        set.insert(FontQuery::new().family("Arial")); // duplicate
        assert_eq!(set.len(), 2);
    }

    // --- FontError ---

    #[test]
    fn font_error_clone_string_variants() {
        let err = FontError::ParseError("clone me".into());
        let err2 = err.clone();
        assert_eq!(err.to_string(), err2.to_string());
    }

    #[test]
    fn font_error_clone_io_variant() {
        let io_err = FontError::from(std::io::Error::new(std::io::ErrorKind::NotFound, "missing"));
        let io_err2 = io_err.clone();
        assert_eq!(io_err.to_string(), io_err2.to_string());
    }

    // --- FontError Display ---

    #[test]
    fn font_error_display_parse() {
        let err = FontError::ParseError("bad header".into());
        assert_eq!(err.to_string(), "font parse error: bad header");
    }

    #[test]
    fn font_error_display_not_found() {
        assert_eq!(FontError::NotFound.to_string(), "font not found");
    }

    #[test]
    fn font_error_display_unsupported() {
        assert_eq!(
            FontError::UnsupportedFormat.to_string(),
            "unsupported font format"
        );
    }

    #[test]
    fn font_error_display_index_out_of_bounds() {
        let err = FontError::IndexOutOfBounds { index: 5, count: 3 };
        assert_eq!(err.to_string(), "face index 5 out of bounds (count=3)");
    }

    // --- ColorGlyphFormat ---

    #[test]
    fn color_glyph_format_equality() {
        assert_eq!(ColorGlyphFormat::ColrV0, ColorGlyphFormat::ColrV0);
        assert_ne!(ColorGlyphFormat::ColrV0, ColorGlyphFormat::Svg);
    }

    // --- GlyphOutline ---

    #[test]
    fn glyph_outline_debug() {
        let cmd = GlyphOutline::MoveTo { x: 1.0, y: 2.0 };
        let debug = format!("{cmd:?}");
        assert!(debug.contains("MoveTo"));
    }

    // --- FontMetrics ---

    #[test]
    fn font_metrics_clone() {
        let m = FontMetrics {
            units_per_em: 1000,
            ascender: 800,
            descender: -200,
            line_gap: 0,
            cap_height: Some(700),
            x_height: Some(500),
            underline_position: -100,
            underline_thickness: 50,
            strikeout_position: 300,
            strikeout_thickness: 50,
        };
        let m2 = m.clone();
        assert_eq!(m, m2);
    }

    // ─── NameTable object-safety test ────────────────────────────────────────

    /// Verify that `NameTable` is object-safe: `Box<dyn NameTable>` must
    /// compile.  Object safety requires that all methods are callable through a
    /// vtable, which holds because both methods return owned `Vec`/`String`
    /// values rather than references with non-`Self` generic parameters.
    #[test]
    fn test_name_table_trait_object_safe() {
        struct DummyNameTable;

        impl NameTable for DummyNameTable {
            fn name_record(&self, _name_id: u16, _language: &str) -> Option<String> {
                None
            }

            fn all_name_records(&self) -> Vec<(u16, String, String)> {
                alloc::vec![]
            }
        }

        let _boxed: Box<dyn NameTable> = Box::new(DummyNameTable);
    }

    // ─── FontCapabilities object-safety test ──────────────────────────────────

    /// Verify that `FontCapabilities` is object-safe: `Box<dyn
    /// FontCapabilities>` must compile.
    ///
    /// Note: `has_feature` has a default implementation that calls
    /// `gsub_features` and `gpos_features`, both returning owned `Vec<[u8;4]>`,
    /// so no RPIT is involved and the trait remains object-safe.
    #[test]
    fn test_font_capabilities_trait_object_safe() {
        struct DummyCapabilities;

        impl FontCapabilities for DummyCapabilities {
            fn gsub_features(&self) -> Vec<[u8; 4]> {
                alloc::vec![]
            }

            fn gpos_features(&self) -> Vec<[u8; 4]> {
                alloc::vec![]
            }

            fn supported_scripts(&self) -> Vec<[u8; 4]> {
                alloc::vec![]
            }

            fn supported_languages(&self, _script: [u8; 4]) -> Vec<[u8; 4]> {
                alloc::vec![]
            }
        }

        let _boxed: Box<dyn FontCapabilities> = Box::new(DummyCapabilities);
    }
}
