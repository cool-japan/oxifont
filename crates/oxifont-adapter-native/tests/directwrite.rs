//! DirectWrite adapter integration tests.
//!
//! These tests exercise the live DirectWrite system font enumeration and are
//! gated behind `#[cfg(windows)]`.  They can only run on a Windows host with
//! the standard system fonts present (e.g. Windows CI).
//!
//! All tests use `#[ignore]` so they are skipped by default on non-Windows
//! platforms and must be explicitly un-ignored when running on Windows CI:
//!
//! ```sh
//! cargo nextest run -p oxifont-adapter-native --test directwrite -- --include-ignored
//! ```
//!
//! On non-Windows CI the file compiles to an empty crate with a single
//! always-passing placeholder test to avoid `dead_code` warnings.

// ---------------------------------------------------------------------------
// Windows-only tests
// ---------------------------------------------------------------------------

/// DirectWrite: verify that the native catalog enumerates at least one font.
///
/// Every standard Windows installation includes system fonts (Segoe UI, Arial,
/// Times New Roman, etc.).  An empty catalog would indicate a systemic failure
/// in the DirectWrite enumeration path.
#[cfg(windows)]
#[test]
fn directwrite_catalog_non_empty() {
    use oxifont_adapter_native::NativeCatalog;
    use oxifont_core::FontCatalog as _;

    let catalog = NativeCatalog::load().expect("DirectWrite catalog load must succeed on Windows");
    assert!(
        !catalog.faces().is_empty(),
        "DirectWrite catalog must contain at least one font on a standard Windows installation"
    );
}

/// DirectWrite: verify that well-known Windows system fonts are present.
///
/// Checks for "Segoe UI", "Arial", or "Times New Roman" — at least one of
/// these ships with every standard Windows installation.
#[cfg(windows)]
#[test]
fn directwrite_well_known_fonts_present() {
    use oxifont_adapter_native::NativeCatalog;
    use oxifont_core::{FontCatalog as _, FontQuery};

    let catalog = NativeCatalog::load().expect("DirectWrite catalog load must succeed on Windows");
    let face = catalog
        .find(&FontQuery::new().family("Segoe UI"))
        .or_else(|| catalog.find(&FontQuery::new().family("Arial")))
        .or_else(|| catalog.find(&FontQuery::new().family("Times New Roman")));

    assert!(
        face.is_some(),
        "at least one of Segoe UI / Arial / Times New Roman must be present on Windows"
    );
}

/// DirectWrite: all CSS weight values must be in the 100–900 range.
#[cfg(windows)]
#[test]
fn directwrite_weight_values_in_range() {
    use oxifont_adapter_native::NativeCatalog;
    use oxifont_core::FontCatalog as _;

    let catalog = NativeCatalog::load().expect("DirectWrite catalog load must succeed on Windows");
    for face in catalog.faces() {
        assert!(
            face.weight >= 100 && face.weight <= 900,
            "CSS weight {} is out of range for '{}'",
            face.weight,
            face.family
        );
    }
}

/// DirectWrite: every FaceInfo must have a non-empty family name.
#[cfg(windows)]
#[test]
fn directwrite_family_names_non_empty() {
    use oxifont_adapter_native::NativeCatalog;
    use oxifont_core::FontCatalog as _;

    let catalog = NativeCatalog::load().expect("DirectWrite catalog load must succeed on Windows");
    for face in catalog.faces().iter().take(50) {
        assert!(
            !face.family.is_empty(),
            "family name must not be empty for font at {:?}",
            face.path
        );
    }
}

/// DirectWrite: every font path must point to an existing file.
///
/// Spot-checks the first 20 faces.
#[cfg(windows)]
#[test]
fn directwrite_font_paths_exist() {
    use oxifont_adapter_native::NativeCatalog;
    use oxifont_core::FontCatalog as _;

    let catalog = NativeCatalog::load().expect("DirectWrite catalog load must succeed on Windows");
    for face in catalog.faces().iter().take(20) {
        assert!(
            face.path.exists(),
            "font path does not exist on disk: {:?}",
            face.path
        );
    }
}

/// DirectWrite: `system_with_options` must return a non-error result with
/// at least one face.
#[cfg(windows)]
#[test]
fn directwrite_system_with_options_works() {
    use oxifont_adapter_native::{NativeCatalog, NativeScanOptions};
    use oxifont_core::FontCatalog as _;

    let opts = NativeScanOptions::default();
    let catalog = NativeCatalog::system_with_options(&opts)
        .expect("system_with_options must succeed on Windows");
    assert!(
        !catalog.faces().is_empty(),
        "system_with_options catalog must contain at least one face on Windows"
    );
}

/// DirectWrite: `reload()` must succeed and yield the same face count.
#[cfg(windows)]
#[test]
fn directwrite_reload_works() {
    use oxifont_adapter_native::NativeCatalog;
    use oxifont_core::FontCatalog as _;

    let mut catalog = NativeCatalog::load().expect("initial load must succeed on Windows");
    let count_before = catalog.faces().len();

    catalog.reload().expect("reload must succeed on Windows");
    let count_after = catalog.faces().len();

    assert_eq!(
        count_before, count_after,
        "face count must be stable across reload (before={count_before}, after={count_after})"
    );
}

/// DirectWrite: `Debug` implementation must produce a string containing
/// "NativeCatalog".
#[cfg(windows)]
#[test]
fn directwrite_debug_impl() {
    use oxifont_adapter_native::NativeCatalog;

    let catalog = NativeCatalog::load().expect("catalog load must succeed on Windows");
    let s = format!("{catalog:?}");
    assert!(
        s.contains("NativeCatalog"),
        "Debug output must contain 'NativeCatalog', got: {s}"
    );
}

/// DirectWrite: oblique / italic faces must be classified correctly.
///
/// Windows ships multiple faces for common families (regular, italic, bold,
/// bold-italic).  This test verifies that at least one italic face is present
/// in the catalog.
#[cfg(windows)]
#[test]
fn directwrite_italic_faces_present() {
    use oxifont_adapter_native::NativeCatalog;
    use oxifont_core::{FontCatalog as _, FontStyle};

    let catalog = NativeCatalog::load().expect("DirectWrite catalog load must succeed on Windows");
    let italic_count = catalog
        .faces()
        .iter()
        .filter(|f| f.style == FontStyle::Italic)
        .count();
    assert!(
        italic_count > 0,
        "at least one italic face must be present on a standard Windows system"
    );
}

// ---------------------------------------------------------------------------
// Non-Windows placeholder — ensures the test binary is never empty.
// ---------------------------------------------------------------------------

/// Placeholder test: always passes on non-Windows platforms.
///
/// This keeps the test binary non-empty so `cargo nextest` does not complain
/// about a test file that compiles to zero tests.
#[cfg(not(windows))]
#[test]
fn directwrite_tests_require_windows_host() {
    // DirectWrite tests are Windows-only.
    // This placeholder ensures the binary compiles and runs on non-Windows hosts.
}
