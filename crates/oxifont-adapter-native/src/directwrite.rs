//! Windows DirectWrite font adapter.
//!
//! Enumerates installed fonts via `DWriteCreateFactory` →
//! `IDWriteFontCollection` → `IDWriteFontFamily` → `IDWriteFont` and builds a
//! [`FontCatalog`](oxifont_core::FontCatalog) from the resulting metadata.
//!
//! All COM interface lifetimes are managed by the `windows` crate wrappers,
//! which implement `Drop` via `Release` automatically. Every `unsafe` block is
//! annotated with a one-line safety explanation.
//!
//! # Weight mapping
//! `DWRITE_FONT_WEIGHT` is a `i32`-newtype whose value is already a CSS
//! weight integer (100–950). We clamp to [100, 900] to stay within the
//! `FaceInfo` range.

#![cfg(windows)]
#![allow(unsafe_code)]

use std::path::PathBuf;

use windows::core::Interface;
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory, IDWriteFont, IDWriteFontCollection, IDWriteFontFace,
    IDWriteFontFamily, IDWriteFontFile, IDWriteLocalFontFileLoader, DWRITE_FACTORY_TYPE_SHARED,
    DWRITE_FONT_STYLE_ITALIC, DWRITE_FONT_STYLE_OBLIQUE,
};

use oxifont_core::{FaceInfo, FontCatalog, FontError, FontQuery, FontStretch, FontStyle};

use crate::NativeError;
use oxifont_parser::ParsedFace;

// ---------------------------------------------------------------------------
// NativeScanOptions
// ---------------------------------------------------------------------------

/// Options for controlling which font directories are enumerated by
/// [`NativeCatalog::system_with_options`].
///
/// The default includes all font sources (user and application fonts).
#[derive(Debug, Clone)]
pub struct NativeScanOptions {
    /// Include per-user font directories (e.g. `%LOCALAPPDATA%\Microsoft\Windows\Fonts`).
    ///
    /// When `false`, only system-wide fonts are included.  DirectWrite's
    /// `GetSystemFontCollection` currently returns all fonts; per-source
    /// filtering is recorded for future implementation via
    /// `IDWriteFontCollection1` / `IDWriteFontSet` APIs.
    pub include_user_fonts: bool,
    /// Include app-bundled fonts registered with the DirectWrite factory.
    ///
    /// Placeholder for future `IDWriteFontSet`-based filtering.
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

/// A font catalog populated by DirectWrite's system font enumeration.
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
            .field("platform", &"DirectWrite (Windows)")
            .finish()
    }
}

/// Performs the actual DirectWrite enumeration and returns a populated `Vec<FaceInfo>`.
///
/// Extracted from `load()` so that `reload()` can reuse the same logic without
/// constructing a whole new `NativeCatalog` value just to destructure it.
fn enumerate_system_faces() -> Result<Vec<FaceInfo>, NativeError> {
    // SAFETY: DWriteCreateFactory is always safe to call on Windows 7+;
    // it manages its own factory lifetime and does not require COM init.
    let factory: IDWriteFactory = unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED) }
        .map_err(|e| NativeError::ComInitFailed(format!("{e:?}")))?;

    // SAFETY: factory is a valid IDWriteFactory; GetSystemFontCollection
    // writes into `collection` on success (BOOL = FALSE = don't check updates).
    let mut collection: Option<IDWriteFontCollection> = None;
    unsafe { factory.GetSystemFontCollection(&mut collection, false) }.map_err(|e| {
        NativeError::DWriteEnumeration(format!("GetSystemFontCollection failed: {e:?}"))
    })?;
    let collection = collection.ok_or_else(|| {
        NativeError::DWriteEnumeration(
            "GetSystemFontCollection returned null collection".to_string(),
        )
    })?;

    // SAFETY: collection is valid; GetFontFamilyCount is a pure read.
    let family_count = unsafe { collection.GetFontFamilyCount() };

    let mut faces = Vec::new();
    for i in 0..family_count {
        if let Ok(family) = unsafe { collection.GetFontFamily(i) } {
            enumerate_family(&family, &mut faces);
        }
    }

    Ok(faces)
}

// ---------------------------------------------------------------------------
// System catalog singleton
// ---------------------------------------------------------------------------

/// Static cache for the system catalog (lazily initialized on first call).
static SYSTEM_CATALOG: std::sync::OnceLock<Option<NativeCatalog>> = std::sync::OnceLock::new();

impl NativeCatalog {
    /// Enumerates all installed fonts via DirectWrite and builds a catalog.
    ///
    /// Fonts that cannot be resolved to an on-disk path (e.g. cloud-backed or
    /// in-memory fonts) are silently skipped.
    ///
    /// # Errors
    /// Returns [`NativeError::ComInitFailed`] if `DWriteCreateFactory` fails, or
    /// [`NativeError::DWriteEnumeration`] if `GetSystemFontCollection` fails —
    /// neither should happen on any Windows 7+ installation.
    pub fn load() -> Result<Self, NativeError> {
        let faces = enumerate_system_faces()?;
        Ok(Self { faces })
    }

    /// Return a cached reference to the system-wide native font catalog.
    ///
    /// The catalog is initialized on the first call and reused on subsequent
    /// calls without re-enumerating system fonts.  Returns `None` if
    /// DirectWrite enumeration fails (unusual on any Windows 7+ installation).
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
    /// On Windows, `GetSystemFontCollection` returns the full set of fonts in a
    /// single unified collection.  The `opts` fields are recorded for
    /// forward-compatibility; the initial implementation is a passthrough to
    /// [`load`](NativeCatalog::load).
    ///
    /// # Errors
    /// Same conditions as [`load`](NativeCatalog::load).
    pub fn system_with_options(_opts: &NativeScanOptions) -> Result<Self, NativeError> {
        // DirectWrite's GetSystemFontCollection does not expose per-source
        // filtering in its v1 API.  Future implementations may use
        // IDWriteFontSet / IDWriteFontCollection1 for finer control.
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
// Internal helpers
// ---------------------------------------------------------------------------

/// Enumerate all faces in one [`IDWriteFontFamily`] and push valid entries.
///
/// COM caching analysis: `GetFamilyNames()` is called **once per family**
/// (not once per face) — the resolved `family_name` and `localized_families`
/// are then passed by reference into `build_face_info` for all faces in the
/// inner `j`-loop.  This is already the optimal structure; no further caching
/// refactor is needed.
fn enumerate_family(family: &IDWriteFontFamily, out: &mut Vec<FaceInfo>) {
    // Retrieve the localized family name list — called ONCE per family.
    // SAFETY: family is a valid IDWriteFontFamily; GetFamilyNames is infallible
    // when called on a system-enumerated family.
    let names = match unsafe { family.GetFamilyNames() } {
        Ok(n) => n,
        Err(_) => return,
    };

    // Read the first (index 0) string from the localized strings object as the
    // canonical family name. Then collect all localized strings for the
    // `localized_families` field.
    let family_name = match read_localized_string(&names, 0) {
        Some(n) => n,
        None => return,
    };

    // Collect all localized family name strings (index 0..GetCount).
    // SAFETY: names is valid; GetCount is a pure read of a stored field.
    let name_count = unsafe { names.GetCount() };
    let localized_families: Vec<String> = (0..name_count)
        .filter_map(|i| read_localized_string(&names, i))
        .filter(|s| !s.is_empty())
        .collect();

    // SAFETY: family deref-coerces to IDWriteFontList; GetFontCount is a pure read.
    let font_count = unsafe { family.GetFontCount() };

    for j in 0..font_count {
        // SAFETY: j is in [0, font_count); GetFont writes an IDWriteFont pointer.
        if let Ok(font) = unsafe { family.GetFont(j) } {
            if let Some(info) = build_face_info(&font, &family_name, &localized_families) {
                out.push(info);
            }
        }
    }
}

/// Build a [`FaceInfo`] from an [`IDWriteFont`], or return `None` if the font
/// cannot be mapped to an on-disk file path.
fn build_face_info(
    font: &IDWriteFont,
    family_name: &str,
    localized_families: &[String],
) -> Option<FaceInfo> {
    // SAFETY: font is a valid IDWriteFont; CreateFontFace is a reference-counted
    // operation that fails gracefully for degenerate fonts.
    let face: IDWriteFontFace = unsafe { font.CreateFontFace() }.ok()?;

    // Retrieve the face index for TTC collections.
    // SAFETY: face is valid; GetIndex is a pure read.
    let face_index = unsafe { face.GetIndex() };

    let path = extract_font_path(&face)?;

    // Only index TTF / OTF / TTC files.
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    if !matches!(ext.as_str(), "ttf" | "otf" | "ttc") {
        return None;
    }

    // SAFETY: font is valid; GetWeight / GetStyle are pure reads of stored fields.
    let dw_weight = unsafe { font.GetWeight() };
    let weight = (dw_weight.0.clamp(100, 900)) as u16;

    // Map DWRITE_FONT_STYLE (Normal = 0, Oblique = 1, Italic = 2) to FontStyle.
    //
    // DWRITE_FONT_STYLE_OBLIQUE fonts are mechanically slanted designs — they
    // must NOT be reported as italic. We map them explicitly to FontStyle::Oblique
    // so that callers can distinguish true italic designs from oblique ones.
    // This matters for CSS font-style matching (italic vs oblique selectors).
    let dw_style = unsafe { font.GetStyle() };
    let style = if dw_style == DWRITE_FONT_STYLE_ITALIC {
        FontStyle::Italic
    } else if dw_style == DWRITE_FONT_STYLE_OBLIQUE {
        // Oblique: mechanically slanted — classified as Oblique, not Italic.
        FontStyle::Oblique
    } else {
        FontStyle::Normal
    };

    // SAFETY: font is valid; GetStretch returns a DWRITE_FONT_STRETCH value (1–9).
    let dw_stretch = unsafe { font.GetStretch() };
    let stretch = FontStretch::from_width_class((dw_stretch.0.clamp(1, 9)) as u8);

    // Extract the PostScript name by parsing the font file at the resolved
    // path and face index.  This avoids a DirectWrite COM round-trip for
    // GetInformationalStrings and reuses the parser already in scope.
    let post_script_name = std::fs::read(&path)
        .ok()
        .and_then(|bytes| ParsedFace::parse(bytes, face_index).ok())
        .and_then(|face| face.postscript_name().map(str::to_string))
        .unwrap_or_default();

    Some(FaceInfo {
        family: std::sync::Arc::from(family_name),
        post_script_name,
        style,
        weight,
        stretch,
        path,
        face_index,
        localized_families: localized_families.to_vec(),
    })
}

/// Attempt to resolve an [`IDWriteFontFace`] to an on-disk [`PathBuf`] via
/// `IDWriteLocalFontFileLoader::GetFilePathFromKey`.
///
/// Returns `None` for cloud-backed or in-memory fonts whose loader does not
/// implement `IDWriteLocalFontFileLoader`.
fn extract_font_path(face: &IDWriteFontFace) -> Option<PathBuf> {
    // Query the number of font files.
    // SAFETY: face is valid; first call with null buf queries the count.
    let mut file_count: u32 = 0;
    unsafe { face.GetFiles(&mut file_count, None) }.ok()?;
    if file_count == 0 {
        return None;
    }

    // Retrieve the file pointers.
    let mut files: Vec<Option<IDWriteFontFile>> = (0..file_count).map(|_| None).collect();
    // SAFETY: face is valid; the vec has exactly file_count slots.
    unsafe { face.GetFiles(&mut file_count, Some(files.as_mut_ptr())) }.ok()?;

    // Use only the first file (composite fonts are rare and unsupported at M4).
    let font_file: &IDWriteFontFile = files.first()?.as_ref()?;

    // Retrieve the opaque reference key used by the loader.
    let mut key_ptr: *mut std::ffi::c_void = std::ptr::null_mut();
    let mut key_size: u32 = 0;
    // SAFETY: font_file is valid; GetReferenceKey stores a pointer into the
    // file object's internal storage — valid for as long as the IDWriteFontFile
    // is alive, which it is (owned by `files` on the stack).
    unsafe { font_file.GetReferenceKey(&mut key_ptr, &mut key_size) }.ok()?;

    // Retrieve the loader and try to cast to IDWriteLocalFontFileLoader.
    // SAFETY: font_file is valid; GetLoader returns a COM pointer.
    let loader = unsafe { font_file.GetLoader() }.ok()?;

    // Cast (QueryInterface) to IDWriteLocalFontFileLoader; fails for non-local.
    // SAFETY: cast() calls QueryInterface, which is always safe on a valid COM obj.
    let local_loader: IDWriteLocalFontFileLoader = loader.cast().ok()?;

    // Query the required path length (in WCHARs, excluding NUL).
    // SAFETY: local_loader is valid; key_ptr is alive (see above).
    let path_len = unsafe { local_loader.GetFilePathLengthFromKey(key_ptr, key_size) }.ok()?;

    // Allocate a buffer and fill it (the API writes len+1 chars including NUL).
    let buf_len = (path_len + 1) as usize;
    let mut wchar_buf: Vec<u16> = vec![0u16; buf_len];
    // SAFETY: local_loader is valid; wchar_buf has capacity buf_len; the API
    // writes exactly path_len chars + a NUL terminator within that range.
    unsafe { local_loader.GetFilePathFromKey(key_ptr, key_size, &mut wchar_buf) }.ok()?;

    // Truncate at the NUL terminator.
    if let Some(nul) = wchar_buf.iter().position(|&c| c == 0) {
        wchar_buf.truncate(nul);
    }

    use std::os::windows::ffi::OsStringExt;
    let os_str = std::ffi::OsString::from_wide(&wchar_buf);
    Some(PathBuf::from(os_str))
}

// ---------------------------------------------------------------------------
// Localized string helper
// ---------------------------------------------------------------------------

/// Read a UTF-16 string at `index` from an [`IDWriteLocalizedStrings`] object.
///
/// Returns `None` on any COM failure or invalid UTF-16 sequence.
fn read_localized_string(
    strings: &windows::Win32::Graphics::DirectWrite::IDWriteLocalizedStrings,
    index: u32,
) -> Option<String> {
    // SAFETY: strings is valid; GetStringLength reads a stored integer field.
    let len = unsafe { strings.GetStringLength(index) }.ok()?;

    let buf_len = (len + 1) as usize; // +1 for the NUL terminator
    let mut buf: Vec<u16> = vec![0u16; buf_len];
    // SAFETY: strings is valid; buf has capacity buf_len; GetString writes at
    // most len+1 wide characters and NUL-terminates within that range.
    unsafe { strings.GetString(index, &mut buf) }.ok()?;

    // Truncate at NUL.
    if let Some(nul) = buf.iter().position(|&c| c == 0) {
        buf.truncate(nul);
    }

    String::from_utf16(&buf).ok()
}
