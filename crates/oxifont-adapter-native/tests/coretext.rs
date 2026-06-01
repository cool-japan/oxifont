//! Integration tests for the CoreText adapter.
//!
//! These tests are compiled and run only on macOS; they exercise the live
//! system font database so they require a macOS host with at least the
//! standard system fonts present.

#[cfg(target_os = "macos")]
mod tests {
    use oxifont_adapter_native::NativeCatalog;
    use oxifont_core::{FontCatalog as _, FontQuery};

    /// The native catalog must contain at least one face on any macOS system.
    #[test]
    fn native_catalog_non_empty() {
        let catalog = NativeCatalog::load().expect("CoreText catalog load failed");
        assert!(
            !catalog.faces().is_empty(),
            "native catalog must contain at least one font face"
        );
    }

    /// A well-known macOS font must be resolvable by family name.
    ///
    /// Tests "Menlo", "Helvetica", and "Arial" in order, accepting whichever
    /// is found first.  Every standard macOS installation includes at least
    /// one of these.
    #[test]
    fn resolve_menlo_or_helvetica() {
        let catalog = NativeCatalog::load().expect("CoreText catalog load failed");
        let face = catalog
            .find(&FontQuery::new().family("Menlo"))
            .or_else(|| catalog.find(&FontQuery::new().family("Helvetica")))
            .or_else(|| catalog.find(&FontQuery::new().family("Arial")))
            .expect("at least one of Menlo / Helvetica / Arial must be present");
        assert!(
            !face.family.is_empty(),
            "matched face must have a non-empty family name"
        );
    }

    /// The path stored in every FaceInfo must point to an existing file.
    ///
    /// We spot-check the first 20 faces to keep the test fast.
    #[test]
    fn face_paths_exist() {
        let catalog = NativeCatalog::load().expect("CoreText catalog load failed");
        for face in catalog.faces().iter().take(20) {
            assert!(
                face.path.exists(),
                "font path does not exist on disk: {:?}",
                face.path
            );
        }
    }

    /// CSS weight values must be in the valid 100–900 range and be multiples
    /// of 100.
    #[test]
    fn weight_values_in_range() {
        let catalog = NativeCatalog::load().expect("CoreText catalog load failed");
        for face in catalog.faces() {
            assert!(
                face.weight >= 100 && face.weight <= 900 && face.weight % 100 == 0,
                "invalid CSS weight {} for font '{}'",
                face.weight,
                face.family
            );
        }
    }
}
