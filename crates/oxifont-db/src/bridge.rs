//! Conversions between `oxifont-db` types and `oxifont-core` types.
//!
//! The two crates model font metadata differently:
//!
//! * `oxifont-db::FaceInfo` — rich database record (source, variable axes,
//!   locale names, PostScript name, etc.).
//! * `oxifont-core::FaceInfo` — slim on-disk descriptor used by higher-level
//!   crates (path, family, style enum, weight, stretch enum, face index).
//!
//! Conversions provided:
//! - [`TryFrom<DbFaceInfo> for CoreFaceInfo`] — db→core; fails when source is
//!   [`Source::Memory`] because `core::FaceInfo` requires an on-disk path.
//! - [`From<CoreFaceInfo> for DbFaceInfo`] — core→db; always succeeds.
//! - [`From<&CoreFaceInfo> for DbFaceInfo`] — borrowing variant.

use oxifont_core::{FaceInfo as CoreFaceInfo, FontStretch, FontStyle, VariationAxis};

use crate::face::{FaceInfo as DbFaceInfo, Source};

// ─── db → core ───────────────────────────────────────────────────────────────

impl TryFrom<DbFaceInfo> for CoreFaceInfo {
    type Error = ();

    /// Convert a database face record to a core face record.
    ///
    /// # Errors
    ///
    /// Returns `Err(())` when the database record's source is
    /// [`Source::Memory`] — a path is required for `core::FaceInfo`.
    fn try_from(f: DbFaceInfo) -> Result<CoreFaceInfo, ()> {
        let path = match f.source {
            Source::File(p) => p,
            Source::Memory(_) => return Err(()),
        };

        // Detect oblique via PostScript name since `db::FaceInfo::italic`
        // merges both italic and oblique into a single boolean.
        let style = if f.italic {
            let psn_lower = f.post_script_name.to_lowercase();
            if psn_lower.contains("oblique") {
                FontStyle::Oblique
            } else {
                FontStyle::Italic
            }
        } else {
            FontStyle::Normal
        };

        let stretch = FontStretch::from_width_class(f.stretch);

        Ok(CoreFaceInfo {
            family: std::sync::Arc::from(f.family.as_str()),
            post_script_name: f.post_script_name,
            style,
            weight: f.weight,
            stretch,
            path,
            face_index: f.face_index,
            localized_families: Vec::new(),
        })
    }
}

// ─── core → db ───────────────────────────────────────────────────────────────

/// Helper shared by the two `From` impls below.
fn core_to_db(core: &CoreFaceInfo) -> DbFaceInfo {
    let italic = matches!(core.style, FontStyle::Italic | FontStyle::Oblique);
    let stretch = core.stretch.to_width_class();

    DbFaceInfo {
        // `id` is assigned by the database when the record is inserted; use 0
        // as a placeholder here.
        id: 0,
        family: core.family.to_string(),
        post_script_name: core.post_script_name.clone(),
        weight: core.weight,
        italic,
        stretch,
        monospaced: false,
        source: Source::File(core.path.clone()),
        face_index: core.face_index,
        // `core::FaceInfo` does not carry variable axes, so the resulting
        // record is treated as a static face.
        variable_axes: Vec::<VariationAxis>::new(),
        locale_families: core
            .localized_families
            .iter()
            .map(|s| (0u16, s.clone()))
            .collect(),
        unicode_ranges: 0,
    }
}

impl From<CoreFaceInfo> for DbFaceInfo {
    /// Convert a core face record to a database face record.
    ///
    /// The resulting record has `id = 0` (a placeholder — the database assigns
    /// the real id on insertion), `monospaced = false`, `variable_axes = []`,
    /// and `unicode_ranges = 0` because `core::FaceInfo` does not carry those
    /// fields.
    fn from(core: CoreFaceInfo) -> Self {
        core_to_db(&core)
    }
}

impl From<&CoreFaceInfo> for DbFaceInfo {
    /// Borrowing variant of [`From<CoreFaceInfo>`] for `DbFaceInfo`.
    fn from(core: &CoreFaceInfo) -> Self {
        core_to_db(core)
    }
}
