//! Serde round-trip tests for `oxifont-core` public types.
//!
//! These tests require the `serde` feature and verify that every serializable
//! type survives a JSON round-trip with identical field values.

#[cfg(feature = "serde")]
mod serde_tests {
    use oxifont_core::{FaceInfo, FontQuery, FontStretch, FontStyle};
    use std::path::PathBuf;
    use std::sync::Arc;

    // -----------------------------------------------------------------------
    // FontStyle round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn font_style_normal_roundtrip() {
        let original = FontStyle::Normal;
        let json = serde_json::to_string(&original).expect("serialize FontStyle::Normal");
        let restored: FontStyle =
            serde_json::from_str(&json).expect("deserialize FontStyle::Normal");
        assert_eq!(original, restored);
    }

    #[test]
    fn font_style_italic_roundtrip() {
        let original = FontStyle::Italic;
        let json = serde_json::to_string(&original).expect("serialize FontStyle::Italic");
        let restored: FontStyle =
            serde_json::from_str(&json).expect("deserialize FontStyle::Italic");
        assert_eq!(original, restored);
    }

    #[test]
    fn font_style_oblique_roundtrip() {
        let original = FontStyle::Oblique;
        let json = serde_json::to_string(&original).expect("serialize FontStyle::Oblique");
        let restored: FontStyle =
            serde_json::from_str(&json).expect("deserialize FontStyle::Oblique");
        assert_eq!(original, restored);
    }

    #[test]
    fn font_style_all_variants_roundtrip() {
        for style in [FontStyle::Normal, FontStyle::Italic, FontStyle::Oblique] {
            let json = serde_json::to_string(&style).expect("serialize FontStyle");
            let restored: FontStyle = serde_json::from_str(&json).expect("deserialize FontStyle");
            assert_eq!(style, restored, "round-trip failed for {style:?}");
        }
    }

    // -----------------------------------------------------------------------
    // FontStretch round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn font_stretch_all_variants_roundtrip() {
        let variants = [
            FontStretch::UltraCondensed,
            FontStretch::ExtraCondensed,
            FontStretch::Condensed,
            FontStretch::SemiCondensed,
            FontStretch::Normal,
            FontStretch::SemiExpanded,
            FontStretch::Expanded,
            FontStretch::ExtraExpanded,
            FontStretch::UltraExpanded,
        ];
        for stretch in variants {
            let json = serde_json::to_string(&stretch).expect("serialize FontStretch");
            let restored: FontStretch =
                serde_json::from_str(&json).expect("deserialize FontStretch");
            assert_eq!(stretch, restored, "round-trip failed for {stretch:?}");
        }
    }

    // -----------------------------------------------------------------------
    // FontQuery round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn font_query_empty_roundtrip() {
        let original = FontQuery::new();
        let json = serde_json::to_string(&original).expect("serialize empty FontQuery");
        let restored: FontQuery = serde_json::from_str(&json).expect("deserialize empty FontQuery");
        assert_eq!(original, restored);
    }

    #[test]
    fn font_query_all_fields_roundtrip() {
        let original = FontQuery::new()
            .family("Noto Sans")
            .style(FontStyle::Italic)
            .weight(700)
            .stretch(FontStretch::SemiCondensed)
            .postscript_name("NotoSans-Italic");

        let json = serde_json::to_string(&original).expect("serialize FontQuery");
        let restored: FontQuery = serde_json::from_str(&json).expect("deserialize FontQuery");
        assert_eq!(original, restored);
    }

    #[test]
    fn font_query_partial_fields_roundtrip() {
        // Only weight and style set — family/stretch/postscript_name remain None.
        let original = FontQuery::new().weight(400).style(FontStyle::Normal);
        let json = serde_json::to_string(&original).expect("serialize partial FontQuery");
        let restored: FontQuery =
            serde_json::from_str(&json).expect("deserialize partial FontQuery");
        assert_eq!(original.weight, restored.weight);
        assert_eq!(original.style, restored.style);
        assert_eq!(restored.family, None);
        assert_eq!(restored.stretch, None);
        assert_eq!(restored.postscript_name, None);
    }

    // -----------------------------------------------------------------------
    // FaceInfo round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn face_info_roundtrip() {
        let original = FaceInfo {
            family: Arc::from("Roboto"),
            post_script_name: "Roboto-Regular".to_string(),
            style: FontStyle::Normal,
            weight: 400,
            stretch: FontStretch::Normal,
            path: PathBuf::from("/usr/share/fonts/Roboto-Regular.ttf"),
            face_index: 0,
            localized_families: vec!["Roboto".to_string()],
        };

        let json = serde_json::to_string(&original).expect("serialize FaceInfo");
        let restored: FaceInfo = serde_json::from_str(&json).expect("deserialize FaceInfo");

        assert_eq!(original.family, restored.family);
        assert_eq!(original.post_script_name, restored.post_script_name);
        assert_eq!(original.style, restored.style);
        assert_eq!(original.weight, restored.weight);
        assert_eq!(original.stretch, restored.stretch);
        assert_eq!(original.path, restored.path);
        assert_eq!(original.face_index, restored.face_index);
        assert_eq!(original.localized_families, restored.localized_families);
    }

    #[test]
    fn face_info_italic_bold_roundtrip() {
        let original = FaceInfo {
            family: Arc::from("Noto Serif"),
            post_script_name: "NotoSerif-BoldItalic".to_string(),
            style: FontStyle::Italic,
            weight: 700,
            stretch: FontStretch::Condensed,
            path: std::env::temp_dir().join("NotoSerif-BoldItalic.ttf"),
            face_index: 0,
            localized_families: vec![],
        };

        let json = serde_json::to_string(&original).expect("serialize bold italic FaceInfo");
        let restored: FaceInfo =
            serde_json::from_str(&json).expect("deserialize bold italic FaceInfo");

        assert_eq!(original.style, restored.style);
        assert_eq!(original.weight, restored.weight);
        assert_eq!(original.stretch, restored.stretch);
    }

    #[test]
    fn face_info_ttc_roundtrip() {
        // TTC/collection font with non-zero face_index.
        let original = FaceInfo {
            family: Arc::from("Source Han Sans"),
            post_script_name: "SourceHanSans-Regular".to_string(),
            style: FontStyle::Normal,
            weight: 400,
            stretch: FontStretch::Normal,
            path: PathBuf::from("/usr/share/fonts/SourceHanSans.ttc"),
            face_index: 2,
            localized_families: vec!["源ノ角ゴシック".to_string(), "本明朝".to_string()],
        };

        let json = serde_json::to_string(&original).expect("serialize TTC FaceInfo");
        let restored: FaceInfo = serde_json::from_str(&json).expect("deserialize TTC FaceInfo");

        assert_eq!(original.face_index, restored.face_index);
        assert_eq!(original.localized_families, restored.localized_families);
    }
}
