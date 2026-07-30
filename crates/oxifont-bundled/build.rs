//! Build script for `oxifont-bundled`.
//!
//! Two independent jobs run here:
//!
//! 1. **CJK font resolution** (always, regardless of the `compressed` feature).
//!    Noto CJK faces are too large (~16 MB each) to vendor into this repository
//!    and are *not* shipped. When a `bundled-noto-cjk-<lang>` feature is enabled,
//!    this script looks for a real, developer-supplied TTF and — only if a valid
//!    SFNT file is found — copies it into `$OUT_DIR/NotoSans<LANG>-Regular.ttf`
//!    and sets the `oxifont_cjk_<lang>_bundled` cfg. When no font is supplied the
//!    feature still compiles, no cfg is set, and the runtime accessor returns a
//!    typed [`FontError::NotFound`] instead of ever handing out empty/fake bytes.
//!
//!    A CJK font can be supplied two ways (checked in this order):
//!      * environment variable `OXIFONT_NOTO_CJK_<LANG>` pointing at a TTF path
//!        (e.g. `OXIFONT_NOTO_CJK_JP=/path/to/NotoSansJP-Regular.ttf`);
//!      * an in-tree file at `fonts/cjk-<lang>/NotoSans<LANG>-Regular.ttf`.
//!
//! 2. **Latin compression** (only when the `compressed` feature is enabled).
//!    Every top-level `.ttf` in `fonts/` is zlib/DEFLATE-compressed with
//!    [`oxiarc_deflate::zlib_compress`] and written to `$OUT_DIR/<name>.ttf.z`;
//!    `catalog.rs` then embeds those compressed bytes.

use std::fs;
use std::path::{Path, PathBuf};

/// A CJK language slot: (lang tag, uppercase tag, cargo feature env, source env var).
const CJK_LANGS: [(&str, &str); 4] = [("jp", "JP"), ("kr", "KR"), ("sc", "SC"), ("tc", "TC")];

fn main() {
    // Always re-run when the fonts directory changes.
    println!("cargo:rerun-if-changed=fonts/");

    // Declare the custom cfgs we may set so that enabling a CJK feature without a
    // supplied font never trips the `unexpected_cfgs` lint (no-warnings policy).
    for (lang, _) in CJK_LANGS {
        println!("cargo:rustc-check-cfg=cfg(oxifont_cjk_{lang}_bundled)");
    }

    let out_dir =
        std::env::var("OUT_DIR").expect("invariant: Cargo always sets OUT_DIR during build");
    let out_path = Path::new(&out_dir);

    resolve_cjk_fonts(out_path);

    // Only perform Latin compression work when the `compressed` feature is active.
    if std::env::var("CARGO_FEATURE_COMPRESSED").is_ok() {
        compress_latin_fonts(out_path);
    }
}

/// Resolve each opt-in CJK font from a developer-supplied source, validate it as
/// a real SFNT, and stage it in `OUT_DIR`. Sets `oxifont_cjk_<lang>_bundled` only
/// when a valid font was staged. Never fabricates or embeds empty/placeholder data.
fn resolve_cjk_fonts(out_path: &Path) {
    for (lang, upper) in CJK_LANGS {
        let env_key = format!("OXIFONT_NOTO_CJK_{upper}");
        println!("cargo:rerun-if-env-changed={env_key}");

        // Only do any work when the corresponding feature is enabled.
        let feature_env = format!("CARGO_FEATURE_BUNDLED_NOTO_CJK_{upper}");
        if std::env::var(&feature_env).is_err() {
            continue;
        }

        let in_tree = PathBuf::from(format!("fonts/cjk-{lang}/NotoSans{upper}-Regular.ttf"));
        if in_tree.exists() {
            println!("cargo:rerun-if-changed={}", in_tree.display());
        }

        let Some((src_path, bytes)) = load_cjk_source(&env_key, &in_tree) else {
            // No font supplied: leave the cfg unset so the runtime accessor
            // returns a typed error. This is the honest "not bundled" path.
            continue;
        };

        if !is_valid_sfnt(&bytes) {
            panic!(
                "build.rs: {} is not a valid TrueType/OpenType font (bad SFNT magic); \
                 refusing to bundle it as NotoSans{upper}-Regular",
                src_path.display()
            );
        }

        let out_file = out_path.join(format!("NotoSans{upper}-Regular.ttf"));
        fs::write(&out_file, &bytes)
            .unwrap_or_else(|e| panic!("build.rs: failed to write {out_file:?}: {e}"));
        println!("cargo:rustc-cfg=oxifont_cjk_{lang}_bundled");
    }
}

/// Load a CJK font from the environment override (highest priority) or the
/// in-tree path. Returns `None` when neither source yields a non-empty file.
///
/// When `env_key` is explicitly set but its file cannot be read, this panics —
/// a wrong developer-supplied path is a build configuration error worth surfacing.
fn load_cjk_source(env_key: &str, in_tree: &Path) -> Option<(PathBuf, Vec<u8>)> {
    if let Ok(path) = std::env::var(env_key) {
        let path = PathBuf::from(path);
        println!("cargo:rerun-if-changed={}", path.display());
        let bytes = fs::read(&path).unwrap_or_else(|e| {
            panic!("build.rs: {env_key} points at {path:?} but it could not be read: {e}")
        });
        if bytes.is_empty() {
            panic!("build.rs: {env_key} points at {path:?} but the file is empty");
        }
        return Some((path, bytes));
    }

    if in_tree.exists() {
        let bytes = fs::read(in_tree)
            .unwrap_or_else(|e| panic!("build.rs: failed to read {in_tree:?}: {e}"));
        if !bytes.is_empty() {
            return Some((in_tree.to_path_buf(), bytes));
        }
    }

    None
}

/// Returns `true` when `bytes` begins with a recognised SFNT signature.
fn is_valid_sfnt(bytes: &[u8]) -> bool {
    if bytes.len() < 4 {
        return false;
    }
    let magic = &bytes[..4];
    magic == [0x00, 0x01, 0x00, 0x00] // TrueType outlines
        || magic == b"OTTO"           // CFF / OpenType CFF
        || magic == b"true"           // legacy Apple TrueType
        || magic == b"ttcf" // TrueType Collection
}

/// Compress every top-level `.ttf` in `fonts/` into `$OUT_DIR/<name>.ttf.z`.
fn compress_latin_fonts(out_path: &Path) {
    let fonts_dir = Path::new("fonts");
    let entries = fs::read_dir(fonts_dir)
        .unwrap_or_else(|e| panic!("build.rs: cannot read fonts/ directory: {e}"));

    for entry in entries {
        let entry = entry.unwrap_or_else(|e| panic!("build.rs: directory entry error: {e}"));
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("ttf") {
            continue;
        }

        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_else(|| panic!("build.rs: non-UTF-8 font file name: {path:?}"));

        let raw =
            fs::read(&path).unwrap_or_else(|e| panic!("build.rs: failed to read {path:?}: {e}"));

        // Level 6 — balanced speed/ratio, same as zlib default.
        let compressed = oxiarc_deflate::zlib_compress(&raw, 6)
            .unwrap_or_else(|e| panic!("build.rs: zlib_compress failed for {file_name}: {e}"));

        let out_file = out_path.join(format!("{file_name}.z"));
        fs::write(&out_file, &compressed)
            .unwrap_or_else(|e| panic!("build.rs: failed to write {out_file:?}: {e}"));
    }
}
