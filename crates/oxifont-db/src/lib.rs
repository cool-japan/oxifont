#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! `oxifont-db` — In-memory indexed font database with CSS Level 4 hybrid
//! query for the OxiFont ecosystem.
//!
//! # Overview
//!
//! This crate provides:
//!
//! * [`FaceInfo`] — lightweight per-face metadata (family, weight, italic,
//!   stretch, variable axes, locale-keyed names).
//! * [`FontDatabase`] — a flat store with secondary family-name index and
//!   optional JSON disk cache (feature `cache`).
//! * [`Query`] — a builder-style CSS Fonts Level 4 matcher augmented with
//!   fontconfig generic-alias resolution and variable-font `wght`-axis
//!   preference.
//!
//! # Quick start
//! ```no_run
//! use oxifont_db::{FontDatabase, Query};
//!
//! let mut db = FontDatabase::new();
//! db.load_dir(std::path::Path::new("/usr/share/fonts")).ok();
//!
//! if let Some(face) = Query::new(&db)
//!     .family("sans-serif")
//!     .weight(700)
//!     .italic(false)
//!     .match_best()
//! {
//!     println!("best match: {} weight={}", face.family, face.weight);
//! }
//! ```
//!
//! # Matching algorithm
//!
//! The CSS Level 4 §4.5 algorithm is used for stretch, style, and weight
//! narrowing.  Generic family names are resolved through a static
//! fontconfig-compatible alias table before the CSS pass.  Variable fonts
//! whose `wght` axis covers the requested weight are preferred over static
//! faces. See [`query`] for full documentation.
//!
//! # Cache strategy
//!
//! Opt-in JSON serialisation is available via `feature = "cache"`.  The
//! cache lives at `$XDG_CACHE_HOME/oxifont/db_v1.json`.  `oxicode` will
//! replace `serde_json` at M3+ when binary encoding is warranted.

mod bridge;
pub mod db;
mod error;
pub mod face;
mod load;
pub mod locale;
pub mod query;

pub use db::{DbStats, FontDatabase};
pub use error::DbError;
pub use face::{FaceInfo, Source, VariationAxis};
/// Deprecated alias: use [`VariationAxis`] from `oxifont_core` instead.
#[deprecated(note = "Use `VariationAxis` (re-exported from `oxifont_core`) instead")]
pub type VariableAxis = VariationAxis;
pub use query::Query;
