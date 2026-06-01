//! Tests for Noto Sans Italic and Noto Sans Mono Regular bundled fonts,
//! and the OnceLock parsed-face cache on `BundledFont`.

#[cfg(feature = "bundled-noto")]
use std::sync::Arc;

#[cfg(feature = "bundled-noto")]
#[test]
fn parsed_face_returns_same_arc_on_repeat_call() {
    use oxifont_bundled::catalog::SANS_REGULAR;

    let a = SANS_REGULAR
        .parsed_face()
        .expect("parsed_face must succeed");
    let b = SANS_REGULAR
        .parsed_face()
        .expect("parsed_face must succeed second time");
    assert!(
        Arc::ptr_eq(&a, &b),
        "must return same Arc on repeated call (OnceLock cache)"
    );
}

#[cfg(feature = "bundled-noto")]
#[test]
fn sans_italic_parses_with_italic_style() {
    use oxifont_bundled::catalog::SANS_ITALIC;
    use oxifont_core::{FontFace as _, FontStyle};

    let face = SANS_ITALIC
        .parsed_face()
        .expect("SANS_ITALIC must parse successfully");
    assert_eq!(
        face.style(),
        FontStyle::Italic,
        "SANS_ITALIC parsed face must report Italic style"
    );
}

#[cfg(feature = "bundled-noto")]
#[test]
fn sans_italic_has_noto_sans_family() {
    use oxifont_bundled::catalog::SANS_ITALIC;
    use oxifont_core::FontFace as _;

    let face = SANS_ITALIC
        .parsed_face()
        .expect("SANS_ITALIC must parse successfully");
    assert_eq!(
        face.family_name(),
        "Noto Sans",
        "SANS_ITALIC must belong to Noto Sans family"
    );
}

#[cfg(feature = "bundled-noto")]
#[test]
fn mono_regular_parses_with_monospace_family() {
    use oxifont_bundled::catalog::MONO_REGULAR;
    use oxifont_core::FontFace as _;

    let face = MONO_REGULAR
        .parsed_face()
        .expect("MONO_REGULAR must parse successfully");
    // The variable-font form of NotoSansMono does not set the OS/2 monospace flag,
    // so we identify it by PostScript name / family name instead.
    let ps_name = face.postscript_name().unwrap_or("");
    assert!(
        ps_name.contains("Mono"),
        "MONO_REGULAR PostScript name must contain 'Mono', got: {ps_name}"
    );
    assert_eq!(
        face.family_name(),
        "Noto Sans Mono",
        "MONO_REGULAR must belong to Noto Sans Mono family"
    );
}

#[cfg(feature = "bundled-noto")]
#[test]
fn mono_regular_descriptor_flags_is_monospace() {
    use oxifont_bundled::catalog::MONO_REGULAR;

    assert!(
        MONO_REGULAR.is_monospace,
        "MONO_REGULAR descriptor must have is_monospace = true"
    );
}

#[cfg(feature = "bundled-noto")]
#[test]
fn bundled_catalog_has_five_faces() {
    use oxifont_bundled::all;

    let count = all().len();
    assert_eq!(
        count, 5,
        "bundled-noto must expose exactly 5 faces \
         (Regular, Bold, Serif Regular, Italic, Mono Regular); got {count}"
    );
}

#[cfg(feature = "bundled-noto")]
#[test]
fn sans_italic_arc_is_cached_independently_of_sans_regular() {
    use oxifont_bundled::catalog::{SANS_ITALIC, SANS_REGULAR};

    let reg = SANS_REGULAR.parsed_face().expect("SANS_REGULAR must parse");
    let ital = SANS_ITALIC.parsed_face().expect("SANS_ITALIC must parse");
    assert!(
        !Arc::ptr_eq(&reg, &ital),
        "SANS_REGULAR and SANS_ITALIC must cache distinct Arcs"
    );
}
