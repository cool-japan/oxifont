#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! `oxifont-bundled` — Bundled SIL-OFL-1.1 Noto font data for the OxiFont ecosystem.
//!
//! This crate ships static byte slices for Noto fonts under the
//! [SIL Open Font License 1.1](https://scripts.sil.org/OFL).
//! All fonts are embedded at compile time via `include_bytes!`.
//!
//! # Feature flags
//!
//! | Feature | Description |
//! |---------|-------------|
//! | `bundled-noto` | Noto Sans Regular/Bold/Italic, Noto Serif Regular, Noto Sans Mono (Latin/Greek/Cyrillic) |
//! | `bundled-noto-cjk-jp` | Accessor for Noto Sans JP Regular — font must be supplied at build time |
//! | `bundled-noto-cjk-kr` | Accessor for Noto Sans KR Regular — font must be supplied at build time |
//! | `bundled-noto-cjk-sc` | Accessor for Noto Sans SC Regular — font must be supplied at build time |
//! | `bundled-noto-cjk-tc` | Accessor for Noto Sans TC Regular — font must be supplied at build time |
//!
//! The Noto CJK faces (~16 MB each) are **not** vendored in this crate. Enabling
//! a `bundled-noto-cjk-*` feature exposes a `noto_sans_<lang>_regular()` accessor
//! that returns the real font bytes when one was supplied at build time (via the
//! `OXIFONT_NOTO_CJK_<LANG>` environment variable or an in-tree
//! `fonts/cjk-<lang>/` file) and a typed [`oxifont_core::FontError::NotFound`]
//! otherwise. It never returns empty or fabricated bytes. See the crate README
//! for the full recipe and CJK licensing.
//!
//! # Quick start
//! ```no_run
//! use oxifont_bundled::provider::BundledFontProvider;
//!
//! let provider = BundledFontProvider::new();
//! for (name, bytes) in provider.font_data() {
//!     println!("{}: {} bytes", name, bytes.len());
//! }
//! ```

pub mod compressed;
pub mod provider;

// ── Bundled Noto (Latin/Greek/Cyrillic) ──────────────────────────────────────

/// Raw bytes of Noto Sans Regular (unhinted TTF, Latin/Greek/Cyrillic).
///
/// Licensed under the SIL Open Font License 1.1.
/// See `../fonts/LICENSE-OFL.txt`.
#[cfg(feature = "bundled-noto")]
pub static NOTO_SANS_REGULAR: &[u8] = include_bytes!("../fonts/NotoSans-Regular.ttf");

/// Raw bytes of Noto Sans Bold (unhinted TTF, Latin/Greek/Cyrillic).
///
/// Licensed under the SIL Open Font License 1.1.
/// See `../fonts/LICENSE-OFL.txt`.
#[cfg(feature = "bundled-noto")]
pub static NOTO_SANS_BOLD: &[u8] = include_bytes!("../fonts/NotoSans-Bold.ttf");

/// Raw bytes of Noto Serif Regular (unhinted TTF, Latin/Greek/Cyrillic).
///
/// Licensed under the SIL Open Font License 1.1.
/// See `../fonts/LICENSE-OFL.txt`.
#[cfg(feature = "bundled-noto")]
pub static NOTO_SERIF_REGULAR: &[u8] = include_bytes!("../fonts/NotoSerif-Regular.ttf");

/// Raw bytes of Noto Sans Italic (variable TTF, Latin/Greek/Cyrillic, weight/width axes).
///
/// This is the variable-font form of Noto Sans Italic sourced from the Google Fonts
/// repository. At face index 0 it resolves to weight 400, italic style.
///
/// Licensed under the SIL Open Font License 1.1.
/// See `../fonts/LICENSE-OFL.txt`.
#[cfg(feature = "bundled-noto")]
pub static NOTO_SANS_ITALIC: &[u8] = include_bytes!("../fonts/NotoSans-Italic.ttf");

/// Raw bytes of Noto Sans Mono Regular (variable TTF, Latin/Greek/Cyrillic, weight/width axes).
///
/// This is the variable-font form of Noto Sans Mono sourced from the Google Fonts
/// repository. At face index 0 it resolves to weight 400, normal style, monospace.
///
/// Licensed under the SIL Open Font License 1.1.
/// See `../fonts/LICENSE-OFL.txt`.
#[cfg(feature = "bundled-noto")]
pub static NOTO_SANS_MONO_REGULAR: &[u8] = include_bytes!("../fonts/NotoSansMono-Regular.ttf");

// ── CJK sub-features ─────────────────────────────────────────────────────────
//
// Noto CJK faces (~16 MB each) are far too large to vendor into this repository,
// so they are NOT shipped. Enabling a `bundled-noto-cjk-<lang>` feature compiles
// an accessor that returns the font bytes *only* when a developer has supplied a
// real TTF at build time (via the `OXIFONT_NOTO_CJK_<LANG>` environment variable
// or an in-tree `fonts/cjk-<lang>/NotoSans<LANG>-Regular.ttf` file — see build.rs).
// When no font is supplied the accessor returns `FontError::NotFound`; it NEVER
// hands out empty or fabricated bytes. See the crate README for the full recipe
// and CJK licensing (SIL OFL 1.1) attribution.
//
// Real fonts are available from: https://github.com/notofonts/noto-cjk/releases

/// Accessor for Noto Sans JP Regular (CJK Unified Ideographs — Japanese).
///
/// Returns the developer-supplied font bytes when one was staged at build time,
/// otherwise [`FontError::NotFound`](oxifont_core::FontError::NotFound). The
/// returned bytes are always a valid SFNT (validated by `build.rs`); an empty or
/// fake slice is never produced.
///
/// To bundle a real font, set `OXIFONT_NOTO_CJK_JP=/path/to/NotoSansJP-Regular.ttf`
/// (or drop the file at `crates/oxifont-bundled/fonts/cjk-jp/NotoSansJP-Regular.ttf`)
/// and build with `--features bundled-noto-cjk-jp`.
///
/// The bundled font, when supplied, is licensed under the SIL Open Font License 1.1.
#[cfg(feature = "bundled-noto-cjk-jp")]
pub fn noto_sans_jp_regular() -> Result<&'static [u8], oxifont_core::FontError> {
    #[cfg(oxifont_cjk_jp_bundled)]
    {
        Ok(include_bytes!(concat!(
            env!("OUT_DIR"),
            "/NotoSansJP-Regular.ttf"
        )))
    }
    #[cfg(not(oxifont_cjk_jp_bundled))]
    {
        Err(oxifont_core::FontError::NotFound)
    }
}

/// Accessor for Noto Sans KR Regular (CJK Unified Ideographs — Korean).
///
/// Returns the developer-supplied font bytes when one was staged at build time,
/// otherwise [`FontError::NotFound`](oxifont_core::FontError::NotFound). See
/// [`noto_sans_jp_regular`] for the supply recipe (env var `OXIFONT_NOTO_CJK_KR`).
///
/// The bundled font, when supplied, is licensed under the SIL Open Font License 1.1.
#[cfg(feature = "bundled-noto-cjk-kr")]
pub fn noto_sans_kr_regular() -> Result<&'static [u8], oxifont_core::FontError> {
    #[cfg(oxifont_cjk_kr_bundled)]
    {
        Ok(include_bytes!(concat!(
            env!("OUT_DIR"),
            "/NotoSansKR-Regular.ttf"
        )))
    }
    #[cfg(not(oxifont_cjk_kr_bundled))]
    {
        Err(oxifont_core::FontError::NotFound)
    }
}

/// Accessor for Noto Sans SC Regular (CJK Unified Ideographs — Simplified Chinese).
///
/// Returns the developer-supplied font bytes when one was staged at build time,
/// otherwise [`FontError::NotFound`](oxifont_core::FontError::NotFound). See
/// [`noto_sans_jp_regular`] for the supply recipe (env var `OXIFONT_NOTO_CJK_SC`).
///
/// The bundled font, when supplied, is licensed under the SIL Open Font License 1.1.
#[cfg(feature = "bundled-noto-cjk-sc")]
pub fn noto_sans_sc_regular() -> Result<&'static [u8], oxifont_core::FontError> {
    #[cfg(oxifont_cjk_sc_bundled)]
    {
        Ok(include_bytes!(concat!(
            env!("OUT_DIR"),
            "/NotoSansSC-Regular.ttf"
        )))
    }
    #[cfg(not(oxifont_cjk_sc_bundled))]
    {
        Err(oxifont_core::FontError::NotFound)
    }
}

/// Accessor for Noto Sans TC Regular (CJK Unified Ideographs — Traditional Chinese).
///
/// Returns the developer-supplied font bytes when one was staged at build time,
/// otherwise [`FontError::NotFound`](oxifont_core::FontError::NotFound). See
/// [`noto_sans_jp_regular`] for the supply recipe (env var `OXIFONT_NOTO_CJK_TC`).
///
/// The bundled font, when supplied, is licensed under the SIL Open Font License 1.1.
#[cfg(feature = "bundled-noto-cjk-tc")]
pub fn noto_sans_tc_regular() -> Result<&'static [u8], oxifont_core::FontError> {
    #[cfg(oxifont_cjk_tc_bundled)]
    {
        Ok(include_bytes!(concat!(
            env!("OUT_DIR"),
            "/NotoSansTC-Regular.ttf"
        )))
    }
    #[cfg(not(oxifont_cjk_tc_bundled))]
    {
        Err(oxifont_core::FontError::NotFound)
    }
}

// ── BundledFont / BundledCatalog ──────────────────────────────────────────────

pub mod catalog;

pub use catalog::{all, BundledCatalog, BundledFont, ALL_FONT_REFS};

#[cfg(feature = "bundled-noto")]
pub use catalog::{MONO_REGULAR, SANS_BOLD, SANS_ITALIC, SANS_REGULAR, SERIF_REGULAR};
