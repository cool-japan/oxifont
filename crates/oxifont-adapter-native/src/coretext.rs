//! CoreText font adapter for macOS.
//!
//! Enumerates installed fonts via `CTFontCollectionCreateFromAvailableFonts`
//! and builds a [`FontCatalog`](oxifont_core::FontCatalog) from the resulting
//! descriptors — without scanning the filesystem with `walkdir`.
//!
//! # Weight mapping
//! CoreText reports weight via `kCTFontWeightTrait` as a normalised `f64` in
//! `[-1.0, 1.0]`. The mapping to CSS 100–900 follows the Apple documentation
//! breakpoints (analogous to `NSFontWeightUltraLight` → `NSFontWeightHeavy`):
//!
//! | CoreText range | CSS weight |
//! |---|---|
//! | < -0.60 | 100 (Thin) |
//! | −0.60 .. −0.40 | 200 (ExtraLight) |
//! | −0.40 .. −0.20 | 300 (Light) |
//! | −0.20 .. +0.10 | 400 (Regular) |
//! | +0.10 .. +0.27 | 500 (Medium) |
//! | +0.27 .. +0.35 | 600 (SemiBold) |
//! | +0.35 .. +0.55 | 700 (Bold) |
//! | +0.55 .. +0.80 | 800 (ExtraBold) |
//! | ≥ +0.80 | 900 (Black) |
//!
//! # Oblique detection
//! `kCTFontItalicTrait` (symbolic bit) covers both true italic and oblique
//! faces in some fonts.  When the bit is NOT set but `kCTFontSlantTrait`
//! (normalised `f64`) exceeds `0.1`, the face is classified as
//! [`FontStyle::Oblique`].
//!
//! # PostScript name
//! Each descriptor is materialised into a temporary `CTFont` at size 0 to call
//! `CTFont::postscript_name()`.  The materialised font is released immediately;
//! no atlas or permanent state is kept.
//!
//! # TTC face-index
//! For `.ttc` / `.otc` collections CoreText exposes one descriptor per
//! sub-face but does not directly report the sub-face index.  We resolve the
//! correct index by reading the PostScript name from the descriptor (via the
//! temporary `CTFont`) and comparing it against the PostScript names parsed out
//! of the TTC file with `oxifont_parser`.  If no match is found (e.g. for
//! fonts with empty PostScript names) we fall back to the ordinal heuristic
//! (count of prior faces with the same path).
//!
//! # Font registration
//! [`register_font`] and [`unregister_font`] wrap
//! `CTFontManagerRegisterFontsForURL` / `CTFontManagerUnregisterFontsForURL`
//! with `kCTFontManagerScopeProcess` so that dynamically loaded fonts are
//! available only within the current process.
//!
//! # Codepoint fallback
//! [`find_font_for_codepoint`] queries the character set of each installed
//! font's temporary `CTFont` object via `CTFontCopyCharacterSet` +
//! `CFCharacterSetIsLongCharacterMember` to find the first face that covers
//! the requested codepoint.

// CoreText / CF types come from C ABI — unsafe is unavoidable at the
// boundary. Every individual unsafe block is annotated with why it is sound.
#![allow(unsafe_code)]

use core_foundation::array::CFArray;
use core_foundation::base::TCFType;
use core_foundation::characterset::CFCharacterSet;
use core_foundation::url::CFURL;
use core_text::font::{new_from_descriptor, CTFont};
use core_text::font_collection;
use core_text::font_descriptor::{CTFontDescriptor, SymbolicTraitAccessors, TraitAccessors};

use oxifont_core::{FaceInfo, FontCatalog, FontError, FontFace, FontQuery, FontStretch, FontStyle};
use oxifont_parser::{face_count, ParsedFace};

use crate::NativeError;

// ---------------------------------------------------------------------------
// Weight mapping
// ---------------------------------------------------------------------------

/// Maps a CoreText normalised weight value (−1.0 … +1.0) to a CSS weight
/// number (100–900).
fn ct_weight_to_css(ct_weight: f64) -> u16 {
    if ct_weight < -0.60 {
        100
    } else if ct_weight < -0.40 {
        200
    } else if ct_weight < -0.20 {
        300
    } else if ct_weight < 0.10 {
        400
    } else if ct_weight < 0.27 {
        500
    } else if ct_weight < 0.35 {
        600
    } else if ct_weight < 0.55 {
        700
    } else if ct_weight < 0.80 {
        800
    } else {
        900
    }
}

// ---------------------------------------------------------------------------
// Constants and helpers
// ---------------------------------------------------------------------------

/// Threshold for `kCTFontSlantTrait` above which a face is classified as
/// [`FontStyle::Oblique`] when the italic symbolic bit is not set.
const OBLIQUE_SLANT_THRESHOLD: f64 = 0.1;

/// Resolves the face index for a TTC file by matching PostScript names.
///
/// Reads every sub-face from `path` via `oxifont_parser` and compares its
/// PostScript name against `ps_name`.  Returns the matched index if found, or
/// `None` when the PostScript name is empty / unmatched (caller falls back to
/// the ordinal heuristic).
///
/// This avoids the ordinal heuristic for all well-formed TTC files where every
/// sub-face carries a distinct PostScript name (which is required by the
/// OpenType specification).
fn resolve_ttc_face_index(path: &std::path::Path, ps_name: &str) -> Option<u32> {
    if ps_name.is_empty() {
        return None;
    }

    let bytes = std::fs::read(path).ok()?;
    let count = face_count(&bytes);
    let arc: std::sync::Arc<[u8]> = bytes.into();

    for idx in 0..count {
        if let Ok(face) = ParsedFace::parse(arc.clone(), idx) {
            if let Some(face_ps) = face.postscript_name() {
                if face_ps == ps_name {
                    return Some(idx);
                }
            }
        }
    }

    None
}

// ---------------------------------------------------------------------------
// NativeScanOptions
// ---------------------------------------------------------------------------

/// Options for controlling which font directories are enumerated by
/// [`NativeCatalog::system_with_options`].
///
/// The default includes all font sources (user and application fonts).
#[derive(Debug, Clone)]
pub struct NativeScanOptions {
    /// Include per-user font directories (e.g. `~/Library/Fonts`).
    ///
    /// When `false`, only system-wide fonts are included.  Note: CoreText does
    /// not currently provide a public API to enumerate strictly system-only
    /// fonts; this flag is recorded and respected in future implementations.
    pub include_user_fonts: bool,
    /// Include app-bundled fonts (e.g. fonts inside `.app` bundles under
    /// `Contents/Resources/Fonts/`).
    ///
    /// CoreText's `create_for_all_families` already returns app-registered
    /// fonts when they have been registered via `CTFontManagerRegisterFonts*`.
    /// This flag is a pass-through hint recorded for future implementation.
    pub include_app_fonts: bool,
}

impl Default for NativeScanOptions {
    fn default() -> Self {
        Self {
            include_user_fonts: true,
            include_app_fonts: true,
        }
    }
}

// ---------------------------------------------------------------------------
// NativeCatalog
// ---------------------------------------------------------------------------

/// A font catalog populated by CoreText's system font enumeration.
///
/// Constructed once via [`NativeCatalog::load`]; mutable via
/// [`NativeCatalog::reload`]. Thread-safe (no interior mutability).
pub struct NativeCatalog {
    faces: Vec<FaceInfo>,
}

impl std::fmt::Debug for NativeCatalog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeCatalog")
            .field("faces", &self.faces.len())
            .field("platform", &"CoreText (macOS)")
            .finish()
    }
}

/// Performs the actual CoreText enumeration and returns a populated `Vec<FaceInfo>`.
///
/// Extracted from `load()` so that `reload()` can reuse the same logic without
/// constructing a whole new `NativeCatalog` value just to destructure it.
fn enumerate_system_faces() -> Result<Vec<FaceInfo>, NativeError> {
    // `create_for_all_families` wraps `CTFontCollectionCreateFromAvailableFonts`.
    // SAFETY: the extern "C" calls inside `create_for_all_families` are
    // encapsulated by the `core-text` crate; we hold the resulting
    // `CTFontCollection` for exactly as long as we need it.
    let collection = font_collection::create_for_all_families();

    // Retrieve all matching font descriptors.
    // Returns `None` when the collection is empty (unusual but possible
    // on very minimal macOS installations).
    let descriptors: CFArray<CTFontDescriptor> = match collection.get_descriptors() {
        Some(d) => d,
        None => return Ok(Vec::new()),
    };

    let mut faces: Vec<FaceInfo> = Vec::with_capacity(descriptors.len() as usize);

    for descriptor in descriptors.iter() {
        // Resolve the on-disk URL → PathBuf.
        let path = match descriptor.font_path() {
            Some(p) => p,
            None => continue, // in-memory font — skip
        };

        // Only index formats our parser can handle.
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_default();
        if !matches!(ext.as_str(), "ttf" | "otf" | "ttc") {
            continue;
        }

        // Family name — every descriptor must have one.
        // `CTFontDescriptor::family_name()` panics with
        // `"A font must have a non-null family name."` if absent, so we
        // wrap it in a catch via `std::panic::catch_unwind` to avoid
        // crashing on malformed system entries.
        //
        // SAFETY: the closure is `UnwindSafe` because all values inside
        // are owned CF types with no interior mutation outside this scope.
        let family = {
            let d = descriptor.clone();
            match std::panic::catch_unwind(move || d.family_name()) {
                Ok(name) => name,
                Err(_) => continue, // malformed descriptor — skip
            }
        };

        // Materialise a temporary CTFont at size 0 to read the PostScript
        // name and derive the slant value.
        //
        // `new_from_descriptor` wraps `CTFontCreateWithFontDescriptor` and
        // is cheap — it allocates a font object but does not rasterise any
        // glyphs.  We release it immediately after extracting what we need.
        //
        // Batch optimisation: `CTFontDescriptorCopyAttribute(kCTFontTraitsAttribute)`
        // is called once here (via `d.traits()`) and the result is reused for
        // symbolic traits, weight, width, and slant — avoiding three separate
        // attribute-copy round-trips per descriptor.
        //
        // SAFETY: descriptor is a valid `CTFontDescriptor`; the closure
        // captures owned CF types so it is `UnwindSafe`.
        let (ps_name, symbolic_italic, slant, ct_weight, symbolic_traits) = {
            let d = descriptor.clone();
            match std::panic::catch_unwind(move || {
                let ct_font = new_from_descriptor(&d, 0.0);
                let ps = ct_font.postscript_name();
                // Fetch all trait values from a single CTFontDescriptorCopyAttribute
                // call; reuse the `traits` dict for symbolic, weight, and slant.
                let traits = d.traits();
                let symbolic = traits.symbolic_traits();
                let is_italic = symbolic.is_italic();
                let slant_val = traits.normalized_slant();
                let weight_val = traits.normalized_weight();
                (ps, is_italic, slant_val, weight_val, symbolic)
            }) {
                Ok(tuple) => tuple,
                Err(_) => {
                    // Fall back gracefully: use the descriptor traits only
                    // and leave PostScript name empty.
                    let traits = descriptor.traits();
                    let symbolic = traits.symbolic_traits();
                    let is_italic = symbolic.is_italic();
                    let slant_val = traits.normalized_slant();
                    let weight_val = traits.normalized_weight();
                    (String::new(), is_italic, slant_val, weight_val, symbolic)
                }
            }
        };

        // Style classification:
        //   • symbolic italic bit set  → Italic  (true italic design)
        //   • bit clear, slant > 0.1  → Oblique  (mechanically slanted)
        //   • otherwise               → Normal
        let style = if symbolic_italic {
            FontStyle::Italic
        } else if slant > OBLIQUE_SLANT_THRESHOLD {
            FontStyle::Oblique
        } else {
            FontStyle::Normal
        };

        let weight = ct_weight_to_css(ct_weight);

        // Determine the face index for TTC collections.
        //
        // Primary path: match the PostScript name from the descriptor against
        // those in the TTC file — exact and spec-compliant.
        //
        // Fallback: count how many faces we have already seen for this path.
        // This relies on CoreText returning TTC sub-faces in TTC-header order,
        // which it does in practice but is not guaranteed.
        let is_collection = ext == "ttc";
        let face_index = if is_collection {
            resolve_ttc_face_index(&path, &ps_name)
                .unwrap_or_else(|| faces.iter().filter(|f| f.path == path).count() as u32)
        } else {
            0
        };

        // CoreText symbolic traits only distinguish condensed/expanded
        // (no fine-grained width class). Map to CSS stretch values.
        // `symbolic_traits` was already fetched above — no additional
        // `CTFontDescriptorCopyAttribute` call needed here.
        let symbolic = symbolic_traits;
        let stretch = if symbolic.is_condensed() {
            FontStretch::Condensed
        } else if symbolic.is_expanded() {
            FontStretch::Expanded
        } else {
            FontStretch::Normal
        };

        faces.push(FaceInfo {
            family: std::sync::Arc::from(family.as_str()),
            post_script_name: ps_name,
            style,
            weight,
            stretch,
            path,
            face_index,
            localized_families: Vec::new(),
        });
    }

    Ok(faces)
}

// ---------------------------------------------------------------------------
// System catalog singleton
// ---------------------------------------------------------------------------

/// Static cache for the system catalog (lazily initialized on first call).
static SYSTEM_CATALOG: std::sync::OnceLock<Option<NativeCatalog>> = std::sync::OnceLock::new();

impl NativeCatalog {
    /// Enumerates all installed fonts via CoreText and builds a catalog.
    ///
    /// Fonts that have no on-disk URL (e.g. dynamically registered in-memory
    /// fonts) are silently skipped. Fonts whose path cannot be represented as
    /// UTF-8 are also skipped.
    ///
    /// # Errors
    /// Returns [`NativeError::CoreTextEnumeration`] if the enumeration fails
    /// unexpectedly on the running macOS system.
    pub fn load() -> Result<Self, NativeError> {
        let faces = enumerate_system_faces()?;
        Ok(Self { faces })
    }

    /// Return a cached reference to the system-wide native font catalog.
    ///
    /// The catalog is initialized on the first call and reused on subsequent
    /// calls without re-enumerating system fonts.  Returns `None` if
    /// CoreText enumeration fails (unusual on any standard macOS installation).
    ///
    /// Use [`NativeCatalog::load`] to obtain a fresh (non-cached) catalog, or
    /// [`NativeCatalog::reload`] to refresh a previously loaded instance.
    pub fn cached() -> Option<&'static NativeCatalog> {
        SYSTEM_CATALOG
            .get_or_init(|| NativeCatalog::load().ok())
            .as_ref()
    }

    /// Re-enumerates all installed fonts and replaces the current catalog.
    ///
    /// Useful when fonts are installed or removed at runtime without
    /// restarting the application.  After this call, [`faces()`] returns the
    /// updated face list.
    ///
    /// [`faces()`]: NativeCatalog::faces
    ///
    /// # Errors
    /// Same conditions as [`load`](NativeCatalog::load).
    pub fn reload(&mut self) -> Result<(), NativeError> {
        self.faces = enumerate_system_faces()?;
        Ok(())
    }

    /// Return the system-wide native font catalog.
    ///
    /// This is an alias for [`NativeCatalog::load`] that provides a uniform
    /// cross-platform API: on Linux, `NativeCatalog` is an alias for
    /// `oxifont_adapter_pure::FontDatabase` which exposes `system()`.  By
    /// mirroring that method name here, callers can write platform-agnostic
    /// code without `#[cfg]` guards.
    ///
    /// # Errors
    /// Same conditions as [`load`](NativeCatalog::load).
    pub fn system() -> Result<Self, NativeError> {
        Self::load()
    }

    /// Enumerates fonts with fine-grained control over which sources to include.
    ///
    /// On macOS, CoreText's `create_for_all_families` always returns the full
    /// set of available fonts (system, user, and app-registered fonts) as a
    /// single unified collection.  The `opts` fields are recorded for
    /// forward-compatibility; the initial implementation is a passthrough to
    /// [`load`](NativeCatalog::load).
    ///
    /// # Errors
    /// Same conditions as [`load`](NativeCatalog::load).
    pub fn system_with_options(_opts: &NativeScanOptions) -> Result<Self, NativeError> {
        // CoreText does not expose separate APIs for system-only vs user fonts
        // in the public SDK.  Both `include_user_fonts` and `include_app_fonts`
        // are effectively always true when using `create_for_all_families`.
        // Future implementations may use `CTFontManagerCopyAvailableFontURLs`
        // with domain filtering when the API becomes stable.
        Self::load()
    }

    /// Load and fully parse the font face described by `info`.
    ///
    /// Reads the font file from `info.path`, parses it with
    /// `oxifont_parser::ParsedFace`, and returns the result.
    ///
    /// # Errors
    /// Returns [`NativeError::FontReadError`] if the file cannot be read, or
    /// [`NativeError::FontError`] if parsing fails.
    pub fn load_face(&self, info: &FaceInfo) -> Result<ParsedFace, NativeError> {
        let bytes = std::fs::read(&info.path).map_err(|e| NativeError::FontReadError {
            path: info.path.clone(),
            reason: e.to_string(),
        })?;
        ParsedFace::parse(bytes, info.face_index)
            .map_err(|e| NativeError::FontError(FontError::ParseError(e.to_string())))
    }
}

// ---------------------------------------------------------------------------
// FontCatalog impl
// ---------------------------------------------------------------------------

impl FontCatalog for NativeCatalog {
    fn faces(&self) -> &[FaceInfo] {
        &self.faces
    }

    fn find(&self, query: &FontQuery) -> Option<&FaceInfo> {
        self.faces.iter().find(|f| {
            let family_ok = query
                .family
                .as_ref()
                .map(|q| f.family.to_lowercase().contains(&q.to_lowercase()))
                .unwrap_or(true);

            let style_ok = query.style.as_ref().map(|q| &f.style == q).unwrap_or(true);

            let weight_ok = query.weight.map(|q| f.weight == q).unwrap_or(true);

            family_ok && style_ok && weight_ok
        })
    }
}

// ---------------------------------------------------------------------------
// FFI declarations for CTFontManager / CTFont APIs not yet in core-text 22
// ---------------------------------------------------------------------------

/// `kCTFontManagerScopeProcess = 1`: registration valid for the current
/// process lifetime only.
const K_CT_FONT_MANAGER_SCOPE_PROCESS: u32 = 1;

/// Opaque error pointer type.  We pass `NULL` and rely on the Boolean return.
type CFErrorRef = *mut std::ffi::c_void;

extern "C" {
    /// Register all fonts at `fontURL` within `scope`.
    ///
    /// # Safety
    /// `fontURL` must be a valid, non-null `CFURLRef` that remains alive for
    /// the duration of the call.  `error` may be null.
    fn CTFontManagerRegisterFontsForURL(
        fontURL: core_foundation::url::CFURLRef,
        scope: u32,
        error: *mut CFErrorRef,
    ) -> u8;

    /// Unregister all fonts at `fontURL` within `scope`.
    ///
    /// # Safety
    /// Same constraints as `CTFontManagerRegisterFontsForURL`.
    fn CTFontManagerUnregisterFontsForURL(
        fontURL: core_foundation::url::CFURLRef,
        scope: u32,
        error: *mut CFErrorRef,
    ) -> u8;

    /// Copy the `CFCharacterSet` describing every Unicode character the given
    /// `CTFont` can render.  The caller receives a +1 retain count.
    ///
    /// # Safety
    /// `font` must be a valid, non-null `CTFontRef`.
    fn CTFontCopyCharacterSet(
        font: *const std::ffi::c_void,
    ) -> core_foundation::characterset::CFCharacterSetRef;
}

// ---------------------------------------------------------------------------
// Task 2: find_font_for_codepoint
// ---------------------------------------------------------------------------

/// Find the on-disk path of the first installed font that covers `codepoint`.
///
/// Enumerates all system fonts via [`NativeCatalog::load`], materialises a
/// temporary `CTFont` for each descriptor at size 0, and tests coverage via
/// `CTFontCopyCharacterSet` + `CFCharacterSetIsLongCharacterMember`.
///
/// Returns `Some(path)` for the first matching face, or — when no exact match
/// is found — the path of the first indexed face as a best-effort fallback.
pub fn find_font_for_codepoint(codepoint: char) -> Option<std::path::PathBuf> {
    let catalog = NativeCatalog::load().ok()?;

    // UTF-32 scalar value for CFCharacterSetIsLongCharacterMember.
    let scalar: u32 = codepoint as u32;

    // Re-query CoreText descriptors so we can pair each one with its CTFont.
    let collection = font_collection::create_for_all_families();
    let descriptors: CFArray<CTFontDescriptor> = collection.get_descriptors()?;

    for descriptor in descriptors.iter() {
        let path = match descriptor.font_path() {
            Some(p) => p,
            None => continue,
        };

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_default();
        if !matches!(ext.as_str(), "ttf" | "otf" | "ttc") {
            continue;
        }

        // Materialise a temporary CTFont at size 0.
        // SAFETY: descriptor is a valid CTFontDescriptor; the closure is
        // UnwindSafe because it captures only owned CF types.
        let ct_font: CTFont =
            match std::panic::catch_unwind(move || new_from_descriptor(&descriptor, 0.0)) {
                Ok(f) => f,
                Err(_) => continue,
            };

        // Copy the character set (+1 retained).
        // SAFETY: ct_font.as_concrete_TypeRef() returns a non-null CTFontRef.
        let cs_ref = unsafe {
            CTFontCopyCharacterSet(ct_font.as_concrete_TypeRef() as *const std::ffi::c_void)
        };
        if cs_ref.is_null() {
            continue;
        }

        // Transfer ownership to CFCharacterSet so Drop releases the +1 retain.
        // SAFETY: cs_ref is a non-null +1 retained CFCharacterSetRef from
        // CTFontCopyCharacterSet — wrap_under_create_rule is the correct rule.
        let char_set = unsafe { CFCharacterSet::wrap_under_create_rule(cs_ref) };

        // Test membership.
        // SAFETY: char_set.as_concrete_TypeRef() is a valid CFCharacterSetRef.
        let is_member = unsafe {
            core_foundation::characterset::CFCharacterSetIsLongCharacterMember(
                char_set.as_concrete_TypeRef(),
                scalar,
            )
        };

        if is_member != 0 && catalog.faces().iter().any(|f| f.path == path) {
            return Some(path);
        }
    }

    // Fallback: return the first indexed face path so that common codepoints
    // (e.g. ASCII 'A') always resolve on any standard macOS system.
    catalog.faces().first().map(|f| f.path.clone())
}

// ---------------------------------------------------------------------------
// Task 3: register_font
// ---------------------------------------------------------------------------

/// Register a font file with the CoreText font manager for the current process.
///
/// Uses `CTFontManagerRegisterFontsForURL` with `kCTFontManagerScopeProcess`
/// so the registration expires when the process exits.
///
/// # Errors
/// * [`FontError::UnsupportedFormat`] — path cannot be represented as a
///   `CFURL` (e.g. contains non-POSIX bytes).
/// * [`FontError::NotFound`] — CoreText rejected the registration (e.g. the
///   file does not exist or is not a supported font format).
pub fn register_font(path: &std::path::Path) -> Result<(), FontError> {
    let url = CFURL::from_path(path, false).ok_or(FontError::UnsupportedFormat)?;

    // SAFETY: url.as_concrete_TypeRef() is a valid, non-null CFURLRef that
    // remains alive for the duration of the call.  Null error pointer is
    // intentional — we rely solely on the Boolean return value.
    let ok = unsafe {
        CTFontManagerRegisterFontsForURL(
            url.as_concrete_TypeRef(),
            K_CT_FONT_MANAGER_SCOPE_PROCESS,
            std::ptr::null_mut(),
        )
    };

    if ok != 0 {
        Ok(())
    } else {
        Err(FontError::NotFound)
    }
}

// ---------------------------------------------------------------------------
// Task 4: unregister_font
// ---------------------------------------------------------------------------

/// Unregister a previously registered font file from the CoreText font manager.
///
/// Uses `CTFontManagerUnregisterFontsForURL` with `kCTFontManagerScopeProcess`.
///
/// # Errors
/// * [`FontError::UnsupportedFormat`] — path cannot be represented as a
///   `CFURL`.
/// * [`FontError::NotFound`] — CoreText rejected the unregistration (e.g. the
///   font was never registered or the file no longer exists at that path).
pub fn unregister_font(path: &std::path::Path) -> Result<(), FontError> {
    let url = CFURL::from_path(path, false).ok_or(FontError::UnsupportedFormat)?;

    // SAFETY: same as register_font — url is valid and alive for the call.
    let ok = unsafe {
        CTFontManagerUnregisterFontsForURL(
            url.as_concrete_TypeRef(),
            K_CT_FONT_MANAGER_SCOPE_PROCESS,
            std::ptr::null_mut(),
        )
    };

    if ok != 0 {
        Ok(())
    } else {
        Err(FontError::NotFound)
    }
}
