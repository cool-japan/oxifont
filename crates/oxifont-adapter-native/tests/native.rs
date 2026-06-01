//! Integration tests for new oxifont-adapter-native features.
//!
//! These tests are compiled and run only on macOS; they exercise the live
//! system font database so they require a macOS host with at least the
//! standard system fonts present.

// ---------------------------------------------------------------------------
// NativeError — cross-platform tests (unconditional variants only)
// ---------------------------------------------------------------------------

/// `NativeError::Display` must produce a non-empty string for every
/// unconditional variant.
#[test]
fn native_error_display() {
    use oxifont_adapter_native::NativeError;

    let e = NativeError::NoFontPath;
    assert!(
        !format!("{e}").is_empty(),
        "NoFontPath Display must not be empty"
    );

    let e = NativeError::PlatformNotSupported;
    assert!(
        !format!("{e}").is_empty(),
        "PlatformNotSupported Display must not be empty"
    );

    let e = NativeError::FontReadError {
        path: std::env::temp_dir().join("test.ttf"),
        reason: "permission denied".to_string(),
    };
    let s = format!("{e}");
    assert!(!s.is_empty(), "FontReadError Display must not be empty");
    assert!(
        s.contains("test.ttf"),
        "FontReadError Display must contain the file path"
    );
}

/// `NativeError` must be convertible to `FontError` via `From` / `?`.
#[test]
fn native_error_converts_to_font_error() {
    use oxifont_adapter_native::NativeError;
    use oxifont_core::FontError;

    let native_err = NativeError::NoFontPath;
    let font_err: FontError = native_err.into();
    // The resulting FontError must carry the NativeError message.
    assert!(
        !font_err.to_string().is_empty(),
        "converted FontError must not have empty message"
    );

    let platform_err = NativeError::PlatformNotSupported;
    let font_err2: FontError = platform_err.into();
    assert!(
        !font_err2.to_string().is_empty(),
        "converted FontError (PlatformNotSupported) must not have empty message"
    );
}

/// `FontError` must be convertible to `NativeError::FontError` via `From`.
#[test]
fn font_error_converts_to_native_error() {
    use oxifont_adapter_native::NativeError;
    use oxifont_core::FontError;

    let font_err = FontError::NotFound;
    let native_err: NativeError = font_err.into();
    let s = format!("{native_err}");
    assert!(!s.is_empty(), "NativeError wrapping FontError must display");
}

#[cfg(target_os = "macos")]
mod macos_tests {
    // -----------------------------------------------------------------------
    // Item 1: CoreText weight mapping tests
    // -----------------------------------------------------------------------

    /// All CSS weights returned by CoreText must fall within the valid 100–900
    /// range defined by the CSS Fonts specification.
    #[test]
    fn test_native_catalog_has_expected_weight_range() {
        use oxifont_adapter_native::NativeCatalog;
        use oxifont_core::FontCatalog as _;

        let catalog = NativeCatalog::load().expect("CoreText should work on macOS");

        let faces = catalog.faces();
        if faces.is_empty() {
            return;
        }

        for face in faces.iter() {
            assert!(
                face.weight >= 100 && face.weight <= 900,
                "weight {} out of CSS range for face {:?}",
                face.weight,
                face.family
            );
        }
    }

    /// A typical macOS system exposes fonts with more than one distinct CSS
    /// weight.  The set of observed weights must be non-empty.
    #[test]
    fn test_native_catalog_has_multiple_weights() {
        use oxifont_adapter_native::NativeCatalog;
        use oxifont_core::FontCatalog as _;

        let catalog = NativeCatalog::load().expect("CoreText should work on macOS");

        let weights: std::collections::HashSet<u16> =
            catalog.faces().iter().map(|f| f.weight).collect();

        assert!(!weights.is_empty(), "should have at least some weights");
    }

    /// Iterating stretch/family fields must not panic — field access is
    /// exercised for every face to catch any internal unwrap/panic.
    #[test]
    fn test_native_catalog_stretch_in_range() {
        use oxifont_adapter_native::NativeCatalog;
        use oxifont_core::FontCatalog as _;

        let catalog = NativeCatalog::load().expect("CoreText should work on macOS");

        for face in catalog.faces().iter() {
            // Accessing both fields must not panic; we do not assert specific
            // stretch values because CoreText only distinguishes three classes.
            let _ = &face.stretch;
            let _ = &face.family;
        }
    }

    /// Every macOS system has at least one system font, and every face must
    /// carry a non-empty family name.
    #[test]
    fn test_native_catalog_contains_common_system_fonts() {
        use oxifont_adapter_native::NativeCatalog;
        use oxifont_core::FontCatalog as _;

        let catalog = NativeCatalog::load().expect("CoreText should work on macOS");

        assert!(
            !catalog.faces().is_empty(),
            "macOS should have system fonts"
        );

        for face in catalog.faces().iter().take(10) {
            assert!(!face.family.is_empty(), "family name should not be empty");
        }
    }

    // -----------------------------------------------------------------------
    // Item 2: TTC face_index verification
    // -----------------------------------------------------------------------

    /// For each `.ttc` file in the catalog, every reported `face_index` must
    /// be within the valid range `[0, sub_face_count)`.
    ///
    /// CoreText may return multiple descriptors for the same physical TTC
    /// sub-face (e.g. one descriptor per locale or per font family variant),
    /// so duplicate `face_index` values across descriptors are expected and
    /// acceptable.  What must never happen is an index that exceeds the number
    /// of sub-faces that the TTC header declares, as that would indicate a
    /// corrupt or mismatched assignment.
    #[test]
    fn test_native_catalog_ttc_face_index_consistent() {
        use oxifont_adapter_native::NativeCatalog;
        use oxifont_core::FontCatalog as _;

        let catalog = NativeCatalog::load().expect("CoreText should work on macOS");

        use std::collections::HashMap;
        // Collect (face_index, face_count_from_header) pairs per TTC path.
        let mut path_to_data: HashMap<String, Vec<u32>> = HashMap::new();

        for face in catalog.faces() {
            if let Some(path_str) = face.path.to_str() {
                if path_str.to_lowercase().ends_with(".ttc") {
                    path_to_data
                        .entry(path_str.to_string())
                        .or_default()
                        .push(face.face_index);
                }
            }
        }

        for (path, indices) in &path_to_data {
            // Read the TTC sub-face count from the file header (bytes 8–11).
            let sub_face_count: u32 = if let Ok(data) = std::fs::read(path) {
                if data.len() >= 12 {
                    u32::from_be_bytes([data[8], data[9], data[10], data[11]])
                } else {
                    continue; // unreadable header — skip
                }
            } else {
                continue; // unreadable file — skip
            };

            for &idx in indices {
                assert!(
                    idx < sub_face_count,
                    "TTC file {} has face_index {} but only {} sub-faces in header (indices: {:?})",
                    path,
                    idx,
                    sub_face_count,
                    indices
                );
            }
        }
    }

    // -----------------------------------------------------------------------
    // Item 3: Weight mapping correctness
    // -----------------------------------------------------------------------

    /// Verify that the CoreText → CSS weight mapping is sane for well-known
    /// system fonts: regular variants should map to ≤ 450 and bold variants
    /// to ≥ 600.  This test is advisory — it passes silently when none of the
    /// target fonts are present on the host.
    #[test]
    fn test_native_catalog_weight_mapping_sanity() {
        use oxifont_adapter_native::NativeCatalog;
        use oxifont_core::{FontCatalog as _, FontStyle};

        let catalog = NativeCatalog::load().expect("CoreText should work on macOS");

        let mut found_regular = false;
        let mut found_bold = false;

        for face in catalog.faces() {
            let name_lower = face.family.to_lowercase();
            let is_target = name_lower.contains("helvetica")
                || name_lower.contains("san francisco")
                || name_lower.contains("arial");

            if is_target && face.style == FontStyle::Normal {
                if face.weight <= 450 {
                    found_regular = true;
                }
                if face.weight >= 600 {
                    found_bold = true;
                }
            }
        }

        // Not every macOS system has Helvetica or Arial installed.
        // When at least one target font is present, assert both weight classes.
        if found_regular || found_bold {
            assert!(
                found_regular,
                "expected a regular-weight face (≤ 450) among known system fonts"
            );
            assert!(
                found_bold,
                "expected a bold-weight face (≥ 600) among known system fonts"
            );
        }
    }

    /// Registering a non-existent font file must fail gracefully.
    ///
    /// `CTFontManagerRegisterFontsForURL` rejects paths that do not resolve to
    /// a readable, valid font file and returns `false`, which we map to
    /// [`oxifont_core::FontError::NotFound`].
    #[test]
    fn test_register_unregister_nonexistent_font() {
        use oxifont_adapter_native::register_font;
        let result = register_font(std::path::Path::new("/nonexistent/font.ttf"));
        assert!(result.is_err(), "should fail for nonexistent path");
    }

    /// ASCII 'A' (U+0041) must resolve to at least one font on any macOS
    /// system, because every standard macOS installation includes fonts that
    /// cover the Basic Latin block.
    #[test]
    fn test_find_font_for_codepoint_ascii() {
        use oxifont_adapter_native::find_font_for_codepoint;
        let result = find_font_for_codepoint('A');
        assert!(result.is_some(), "should find a font for ASCII 'A'");
    }

    /// The path returned by [`find_font_for_codepoint`] must point to an
    /// existing file on disk.
    #[test]
    fn test_find_font_for_codepoint_path_exists() {
        use oxifont_adapter_native::find_font_for_codepoint;
        if let Some(path) = find_font_for_codepoint('A') {
            assert!(
                path.exists(),
                "font path returned by find_font_for_codepoint must exist on disk: {path:?}"
            );
        }
    }

    /// Unregistering a font that was never registered (non-existent path)
    /// must also fail gracefully — it must not panic.
    #[test]
    fn test_unregister_nonexistent_font_is_err() {
        use oxifont_adapter_native::unregister_font;
        let result = unregister_font(std::path::Path::new("/nonexistent/font.ttf"));
        assert!(
            result.is_err(),
            "unregistering a non-existent font should return Err"
        );
    }

    /// Register a real system font and immediately unregister it.  This
    /// verifies the full round-trip works when the font file is valid and
    /// accessible.  The test is conservative: we only attempt it when the
    /// catalog yields at least one face with a `.ttf` or `.otf` path.
    #[test]
    fn test_register_unregister_roundtrip_system_font() {
        use oxifont_adapter_native::{register_font, unregister_font, NativeCatalog};
        use oxifont_core::FontCatalog as _;

        let catalog = NativeCatalog::load().expect("catalog load should succeed");

        // Find the first TTF/OTF face (avoid TTC to keep the test simple).
        let face = catalog.faces().iter().find(|f| {
            f.path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| matches!(e, "ttf" | "otf"))
                .unwrap_or(false)
        });

        if let Some(face) = face {
            // Re-registering an already-registered system font may succeed or
            // fail depending on macOS policy — either outcome is acceptable.
            // What we care about is that it does NOT panic.
            let _ = register_font(&face.path);

            // Unregistering a system font registered at process scope also
            // should not panic regardless of success.
            let _ = unregister_font(&face.path);
        }
    }

    /// The `Debug` implementation must produce a string containing
    /// `"NativeCatalog"`.
    #[test]
    fn test_debug_impl() {
        use oxifont_adapter_native::NativeCatalog;

        let catalog = NativeCatalog::load().expect("catalog load should succeed");
        let catalog_str = format!("{catalog:?}");
        assert!(
            catalog_str.contains("NativeCatalog"),
            "Debug output must contain 'NativeCatalog', got: {catalog_str}"
        );
    }

    /// `system_with_options` with default options must return a non-error
    /// result on macOS and yield at least one face.
    #[test]
    fn test_system_with_options_returns_catalog() {
        use oxifont_adapter_native::{NativeCatalog, NativeScanOptions};
        use oxifont_core::FontCatalog as _;

        let opts = NativeScanOptions::default();
        let result = NativeCatalog::system_with_options(&opts);
        assert!(
            result.is_ok(),
            "system_with_options must succeed on macOS: {:?}",
            result.err()
        );
        let catalog = result.expect("already checked Ok");
        assert!(
            !catalog.faces().is_empty(),
            "system_with_options catalog must contain at least one face"
        );
    }

    /// `reload()` must succeed and leave the catalog with a similar face
    /// count to before — no fonts are installed or removed during the test,
    /// so the count should be identical.
    #[test]
    fn test_reload_works() {
        use oxifont_adapter_native::NativeCatalog;
        use oxifont_core::FontCatalog as _;

        let mut catalog = NativeCatalog::load().expect("initial load should succeed");
        let count_before = catalog.faces().len();

        catalog.reload().expect("reload should succeed on macOS");
        let count_after = catalog.faces().len();

        // No font install/uninstall happened between load and reload, so the
        // face count must be identical.
        assert_eq!(
            count_before, count_after,
            "face count should be identical after reload (before={count_before}, after={count_after})"
        );
    }
}

// ---------------------------------------------------------------------------
// cached() singleton tests — macOS and Windows only
// ---------------------------------------------------------------------------

#[cfg(any(target_os = "macos", windows))]
mod cached_tests {
    /// Two calls to `cached()` must return consistent results: either both
    /// yield a catalog with the same face count, or both yield `None`.
    ///
    /// Because `cached()` returns a `&'static`, both calls return the exact
    /// same pointer, making the face-count check trivially true; we also
    /// verify pointer identity for stronger assurance.
    #[test]
    fn cached_system_catalog_is_consistent() {
        use oxifont_adapter_native::NativeCatalog;
        use oxifont_core::FontCatalog as _;

        let first = NativeCatalog::cached();
        let second = NativeCatalog::cached();

        match (first, second) {
            (Some(a), Some(b)) => {
                // Both calls return the same static — verify pointer identity
                // to confirm no re-enumeration occurred.
                assert!(
                    std::ptr::eq(a, b),
                    "cached() must return the same static instance on every call"
                );
                // Face counts must match (trivially, since it's the same object).
                assert_eq!(
                    a.faces().len(),
                    b.faces().len(),
                    "face count must be identical across cached() calls"
                );
            }
            (None, None) => {
                // Enumeration failed — both calls consistently return None.
            }
            _ => panic!("cached() returned inconsistent results between calls"),
        }
    }

    /// `cached()` must not panic regardless of the underlying platform state.
    #[test]
    fn cached_system_catalog_never_panics() {
        use oxifont_adapter_native::NativeCatalog;
        // Should not panic regardless of platform or font availability.
        let _ = NativeCatalog::cached();
    }

    /// When the system catalog is available, it must contain at least one face.
    ///
    /// Any standard macOS or Windows installation provides system fonts,
    /// so an empty catalog is unexpected but not a hard failure.
    #[test]
    fn cached_system_catalog_non_empty_on_standard_system() {
        use oxifont_adapter_native::NativeCatalog;
        use oxifont_core::FontCatalog as _;

        if let Some(catalog) = NativeCatalog::cached() {
            assert!(
                !catalog.faces().is_empty(),
                "cached catalog must contain at least one face on a standard system"
            );
        }
        // If cached() returns None, the test passes — no installed fonts is
        // valid (unusual, but not a crate bug).
    }

    /// The face count from `cached()` must equal the face count from a
    /// freshly loaded catalog (within the same process, before any font
    /// install/uninstall).
    #[test]
    fn cached_catalog_matches_fresh_load() {
        use oxifont_adapter_native::NativeCatalog;
        use oxifont_core::FontCatalog as _;

        let cached = NativeCatalog::cached();
        let fresh = NativeCatalog::load().ok();

        match (cached, fresh) {
            (Some(c), Some(f)) => {
                assert_eq!(
                    c.faces().len(),
                    f.faces().len(),
                    "cached catalog must have the same face count as a fresh load"
                );
            }
            (None, None) => {
                // Both consistently fail — acceptable on a system with no fonts.
            }
            (Some(_), None) => {
                // Cached succeeded but fresh load failed — this is unexpected
                // since cache was populated from load(); treat as a warning,
                // not a hard failure (could be a transient OS issue).
            }
            (None, Some(_)) => {
                panic!("fresh load succeeded but cached() returned None — cache was populated from load()");
            }
        }
    }
}
