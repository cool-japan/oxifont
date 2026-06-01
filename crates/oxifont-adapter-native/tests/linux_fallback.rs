//! Tests that the native adapter gracefully falls back on non-native platforms.
//!
//! On Linux (and other non-macOS, non-Windows platforms), `NativeCatalog` is
//! re-exported from `oxifont_adapter_pure::FontDatabase`, which scans
//! filesystem font directories without requiring libfontconfig.

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
#[test]
fn native_catalog_on_linux_returns_ok() {
    use oxifont_adapter_native::NativeCatalog;

    // On Linux, NativeCatalog::system() should succeed (using the stub/discovery fallback)
    // and return Ok — even if it finds zero system fonts.
    let result = NativeCatalog::system();
    assert!(
        result.is_ok(),
        "NativeCatalog::system() must not error on Linux: {:?}",
        result.err()
    );
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
#[test]
fn native_catalog_faces_accessible_on_linux() {
    use oxifont_adapter_native::NativeCatalog;
    use oxifont_core::FontCatalog as _;

    if let Ok(catalog) = NativeCatalog::system() {
        // Just verify the API is callable — face count may be 0 in CI
        let _count = catalog.faces().len();
    }
}
