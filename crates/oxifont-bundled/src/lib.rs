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
//! | `bundled-noto` | Noto Sans Regular/Bold and Noto Serif Regular (Latin/Greek/Cyrillic) |
//! | `bundled-noto-cjk-jp` | Noto Sans JP Regular (requires `bundled-noto`) |
//! | `bundled-noto-cjk-kr` | Noto Sans KR Regular (requires `bundled-noto`) |
//! | `bundled-noto-cjk-sc` | Noto Sans SC Regular (requires `bundled-noto`) |
//! | `bundled-noto-cjk-tc` | Noto Sans TC Regular (requires `bundled-noto`) |
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
// NOTE: These placeholder files are intentionally zero-byte stubs.
// To use CJK fonts, place the real NotoSansJP/KR/SC/TC Regular TTF files at
// the paths indicated below and rebuild with the corresponding feature flag.
// Real fonts are available from: https://github.com/notofonts/noto-cjk/releases

/// Raw bytes of Noto Sans JP Regular (CJK Unified Ideographs — Japanese).
///
/// **Placeholder**: the bundled file is empty by default.
/// To enable, place `NotoSansJP-Regular.ttf` in `crates/oxifont-bundled/fonts/cjk-jp/`
/// and rebuild with `--features bundled-noto-cjk-jp`.
///
/// Licensed under the SIL Open Font License 1.1.
#[cfg(feature = "bundled-noto-cjk-jp")]
pub static NOTO_SANS_JP_REGULAR: &[u8] = include_bytes!("../fonts/cjk-jp/NotoSansJP-Regular.ttf");

/// Raw bytes of Noto Sans KR Regular (CJK Unified Ideographs — Korean).
///
/// **Placeholder**: the bundled file is empty by default.
/// To enable, place `NotoSansKR-Regular.ttf` in `crates/oxifont-bundled/fonts/cjk-kr/`
/// and rebuild with `--features bundled-noto-cjk-kr`.
///
/// Licensed under the SIL Open Font License 1.1.
#[cfg(feature = "bundled-noto-cjk-kr")]
pub static NOTO_SANS_KR_REGULAR: &[u8] = include_bytes!("../fonts/cjk-kr/NotoSansKR-Regular.ttf");

/// Raw bytes of Noto Sans SC Regular (CJK Unified Ideographs — Simplified Chinese).
///
/// **Placeholder**: the bundled file is empty by default.
/// To enable, place `NotoSansSC-Regular.ttf` in `crates/oxifont-bundled/fonts/cjk-sc/`
/// and rebuild with `--features bundled-noto-cjk-sc`.
///
/// Licensed under the SIL Open Font License 1.1.
#[cfg(feature = "bundled-noto-cjk-sc")]
pub static NOTO_SANS_SC_REGULAR: &[u8] = include_bytes!("../fonts/cjk-sc/NotoSansSC-Regular.ttf");

/// Raw bytes of Noto Sans TC Regular (CJK Unified Ideographs — Traditional Chinese).
///
/// **Placeholder**: the bundled file is empty by default.
/// To enable, place `NotoSansTC-Regular.ttf` in `crates/oxifont-bundled/fonts/cjk-tc/`
/// and rebuild with `--features bundled-noto-cjk-tc`.
///
/// Licensed under the SIL Open Font License 1.1.
#[cfg(feature = "bundled-noto-cjk-tc")]
pub static NOTO_SANS_TC_REGULAR: &[u8] = include_bytes!("../fonts/cjk-tc/NotoSansTC-Regular.ttf");

// ── BundledFont / BundledCatalog ──────────────────────────────────────────────

pub mod catalog;

pub use catalog::{all, BundledCatalog, BundledFont, ALL_FONT_REFS};

#[cfg(feature = "bundled-noto")]
pub use catalog::{MONO_REGULAR, SANS_BOLD, SANS_ITALIC, SANS_REGULAR, SERIF_REGULAR};
