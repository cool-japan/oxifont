//! Correctness tests for the CoreText enumeration path.
//!
//! These tests run only on macOS and exercise the live system font database.

#[cfg(target_os = "macos")]
#[test]
fn coretext_enumerate_returns_faces() {
    use oxifont_adapter_native::NativeCatalog;
    use oxifont_core::FontCatalog as _;

    let catalog = NativeCatalog::system().expect("CoreText enumeration must succeed on macOS");
    assert!(
        !catalog.faces().is_empty(),
        "macOS system must have at least one font"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn coretext_enumerate_faces_have_valid_families() {
    use oxifont_adapter_native::NativeCatalog;
    use oxifont_core::FontCatalog as _;

    let catalog = NativeCatalog::system().expect("CoreText enumeration must succeed");
    for face in catalog.faces().iter().take(10) {
        assert!(
            !face.family.is_empty(),
            "all faces must have non-empty family names"
        );
        assert!(face.path.exists(), "face path must exist: {:?}", face.path);
    }
}

/// Verify that the enumeration doesn't panic on macOS even when called repeatedly.
///
/// Note: this test exercises the happy path — it confirms repeated calls
/// succeed without panicking.  The `catch_unwind` path inside `enumerate_system_faces`
/// protects against malformed descriptors from the OS; injecting a synthetic
/// malformed descriptor would require internal test hooks not exposed by the
/// public API.  See TODO.md for the deferred catch_unwind injection test.
#[cfg(target_os = "macos")]
#[test]
fn catch_unwind_protects_enumeration() {
    use oxifont_adapter_native::NativeCatalog;
    use oxifont_core::FontCatalog as _;

    for _ in 0..3 {
        let catalog = NativeCatalog::system().expect("enumeration must not panic");
        assert!(!catalog.faces().is_empty());
    }
}
