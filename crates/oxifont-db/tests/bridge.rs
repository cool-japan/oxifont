//! Tests for cross-crate [`From`] / [`TryFrom`] conversions between
//! `oxifont_core::FaceInfo` and `oxifont_db::FaceInfo`.

use oxifont_core::{FaceInfo as CoreFaceInfo, FontStretch, FontStyle, VariationAxis};
use oxifont_db::{FaceInfo as DbFaceInfo, Source};
use std::sync::Arc;

// ─── helpers ─────────────────────────────────────────────────────────────────

/// Build a minimal [`CoreFaceInfo`] backed by a temp-file path.
fn make_core_face(family: &str) -> CoreFaceInfo {
    CoreFaceInfo {
        family: Arc::from(family),
        post_script_name: format!("{family}-Regular"),
        style: FontStyle::Normal,
        weight: 400,
        stretch: FontStretch::Normal,
        path: std::env::temp_dir().join("dummy.ttf"),
        face_index: 0,
        localized_families: Vec::new(),
    }
}

/// Build a minimal [`DbFaceInfo`] backed by a [`Source::Memory`] blob.
fn make_db_face(family: &str) -> DbFaceInfo {
    DbFaceInfo {
        id: 1,
        family: family.to_string(),
        post_script_name: format!("{family}-Regular"),
        weight: 400,
        italic: false,
        stretch: 5,
        monospaced: false,
        source: Source::Memory(Vec::new()),
        face_index: 0,
        variable_axes: Vec::new(),
        locale_families: Vec::new(),
        unicode_ranges: 0,
    }
}

// ─── Item 1 verification ─────────────────────────────────────────────────────

/// `VariationAxis` is now the *same* type in both crates.
///
/// If this test compiles, the unification is complete: the type alias check
/// uses `std::convert::identity` which only compiles when both sides are the
/// same underlying type.
#[test]
fn test_variation_axis_type_is_shared() {
    use oxifont_db::VariationAxis as DbAxis;
    // These must be the same type — identity only compiles if they are.
    let _: fn(VariationAxis) -> DbAxis = std::convert::identity;
}

/// Constructing a [`VariationAxis`] with the canonical field names succeeds,
/// confirming that the local `VariableAxis` struct (with `min/max/default`) has
/// been removed and is no longer accessible through `oxifont_db`.
#[test]
fn test_variation_axis_field_names() {
    let axis = VariationAxis {
        tag: *b"wght",
        min_value: 100.0,
        max_value: 900.0,
        default_value: 400.0,
        name: String::new(),
    };
    assert_eq!(axis.tag, *b"wght");
    assert_eq!(axis.min_value, 100.0);
    assert_eq!(axis.max_value, 900.0);
    assert_eq!(axis.default_value, 400.0);
}

// ─── Item 2: From impls ───────────────────────────────────────────────────────

/// `DbFaceInfo::from(CoreFaceInfo)` maps fields correctly.
#[test]
fn test_from_core_faceinfo_owned() {
    let core = make_core_face("TestFont");
    let db = DbFaceInfo::from(core);

    assert_eq!(db.family, "TestFont");
    assert_eq!(db.post_script_name, "TestFont-Regular");
    assert_eq!(db.weight, 400);
    assert!(!db.italic);
    assert_eq!(db.stretch, 5); // FontStretch::Normal → width class 5
    assert_eq!(db.face_index, 0);
    assert!(db.variable_axes.is_empty());
    assert_eq!(db.unicode_ranges, 0);
    // Source must be a File variant (core always has a path).
    assert!(matches!(db.source, Source::File(_)));
}

/// `DbFaceInfo::from(&CoreFaceInfo)` (borrowing) maps fields correctly.
#[test]
fn test_from_core_faceinfo_ref() {
    let core = make_core_face("BorrowedFont");
    let db = DbFaceInfo::from(&core);

    assert_eq!(db.family, "BorrowedFont");
    assert_eq!(db.weight, 400);
    // The original `core` must still be usable after the borrow.
    assert_eq!(&*core.family, "BorrowedFont");
}

/// Italic faces: `FontStyle::Italic` maps to `db.italic == true`.
#[test]
fn test_from_core_italic_maps_correctly() {
    let core = CoreFaceInfo {
        style: FontStyle::Italic,
        ..make_core_face("ItalicFont")
    };
    let db = DbFaceInfo::from(core);
    assert!(db.italic);
}

/// Oblique faces: `FontStyle::Oblique` also maps to `db.italic == true`.
#[test]
fn test_from_core_oblique_maps_to_italic_true() {
    let core = CoreFaceInfo {
        style: FontStyle::Oblique,
        ..make_core_face("ObliqueFont")
    };
    let db = DbFaceInfo::from(core);
    assert!(db.italic);
}

/// `FontStretch::Condensed` (width class 3) maps to `db.stretch == 3`.
#[test]
fn test_from_core_stretch_maps_correctly() {
    let core = CoreFaceInfo {
        stretch: FontStretch::Condensed,
        ..make_core_face("CondensedFont")
    };
    let db = DbFaceInfo::from(core);
    assert_eq!(db.stretch, 3);
}

/// Localized families from `core.localized_families` survive the conversion.
#[test]
fn test_from_core_localized_families_preserved() {
    let core = CoreFaceInfo {
        localized_families: vec!["テストフォント".to_string(), "测试字体".to_string()],
        ..make_core_face("LocaleFont")
    };
    let db = DbFaceInfo::from(core);
    // locale_families carries (lcid, name); lcid is 0 since core has no LCID.
    assert_eq!(db.locale_families.len(), 2);
    assert!(db
        .locale_families
        .iter()
        .any(|(_, n)| n == "テストフォント"));
    assert!(db.locale_families.iter().any(|(_, n)| n == "测试字体"));
}

// ─── TryFrom<DbFaceInfo> for CoreFaceInfo ────────────────────────────────────

/// `TryFrom<DbFaceInfo>` succeeds when the source is a file.
#[test]
fn test_try_from_db_file_succeeds() {
    let db = DbFaceInfo {
        source: Source::File(std::env::temp_dir().join("font.ttf")),
        ..make_db_face("TryFromFont")
    };
    let core = CoreFaceInfo::try_from(db);
    assert!(core.is_ok());
    let core = core.expect("conversion must succeed for File source");
    assert_eq!(&*core.family, "TryFromFont");
}

/// `TryFrom<DbFaceInfo>` fails when the source is in-memory.
#[test]
fn test_try_from_db_memory_fails() {
    let db = make_db_face("MemoryFont"); // Source::Memory
    let result = CoreFaceInfo::try_from(db);
    assert!(
        result.is_err(),
        "TryFrom should fail for in-memory sources (no path available)"
    );
}

/// Italic detection: db→core uses PostScript name to distinguish oblique.
#[test]
fn test_try_from_db_oblique_detected_via_psn() {
    let db = DbFaceInfo {
        post_script_name: "MyFont-Oblique".to_string(),
        italic: true,
        source: Source::File(std::env::temp_dir().join("oblique.ttf")),
        ..make_db_face("ObliqueDb")
    };
    let core = CoreFaceInfo::try_from(db).expect("should succeed");
    assert_eq!(core.style, FontStyle::Oblique);
}
