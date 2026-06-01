//! `oxifont-adapter-native` — OS-native font adapter for OxiFont.
//!
//! Provides [`NativeCatalog`], a [`FontCatalog`](oxifont_core::FontCatalog)
//! implementation that uses the operating system's own font enumeration API:
//!
//! | Platform | API used |
//! |----------|----------|
//! | macOS    | CoreText (`CTFontCollection`) |
//! | Windows  | DirectWrite (`IDWriteFontCollection`) via `windows` 0.62 |
//! | other    | Alias for `oxifont_adapter_pure::FontDatabase` |
//!
//! # Example (macOS)
//! ```no_run
//! use oxifont_adapter_native::NativeCatalog;
//! use oxifont_core::{FontCatalog as _, FontQuery};
//!
//! let catalog = NativeCatalog::load().expect("native catalog");
//! println!("found {} faces via CoreText", catalog.faces().len());
//! if let Some(face) = catalog.find(&FontQuery::new().family("Helvetica")) {
//!     println!("found: {}", face.family);
//! }
//! ```

// Silence the unused-import warning that `oxifont_core` triggers on the
// non-macOS/non-windows branch (where `FontDatabase` re-export brings in all
// required items indirectly).
#[allow(unused_imports)]
use oxifont_core as _;

mod error;
pub use error::NativeError;

#[cfg(target_os = "macos")]
mod coretext;
#[cfg(target_os = "macos")]
pub use coretext::NativeCatalog;
#[cfg(target_os = "macos")]
pub use coretext::NativeScanOptions;
#[cfg(target_os = "macos")]
pub use coretext::{find_font_for_codepoint, register_font, unregister_font};

#[cfg(windows)]
mod directwrite;
#[cfg(windows)]
pub use directwrite::NativeCatalog;
#[cfg(windows)]
pub use directwrite::NativeScanOptions;

#[cfg(not(any(target_os = "macos", windows)))]
pub use oxifont_adapter_pure::FontDatabase as NativeCatalog;
