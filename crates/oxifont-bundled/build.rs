//! Build script for `oxifont-bundled`.
//!
//! When the `compressed` feature is enabled, this script reads every `.ttf`
//! file from the `fonts/` directory (non-recursively), compresses it with
//! zlib/DEFLATE using [`oxiarc_deflate::zlib_compress`], and writes the result
//! to `$OUT_DIR/<name>.ttf.z`.  The catalog module then embeds those
//! compressed bytes with `include_bytes!(concat!(env!("OUT_DIR"), "/<name>.ttf.z"))`.
//!
//! When the `compressed` feature is **not** enabled nothing is written; the
//! catalog uses plain `include_bytes!("../fonts/<name>.ttf")` paths.

use std::fs;
use std::path::Path;

fn main() {
    // Always re-run when the fonts directory changes.
    println!("cargo:rerun-if-changed=fonts/");

    // Only perform compression work when the `compressed` feature is active.
    if std::env::var("CARGO_FEATURE_COMPRESSED").is_err() {
        return;
    }

    let out_dir =
        std::env::var("OUT_DIR").expect("invariant: Cargo always sets OUT_DIR during build");
    let out_path = Path::new(&out_dir);

    let fonts_dir = Path::new("fonts");
    let entries = fs::read_dir(fonts_dir).unwrap_or_else(|e| {
        panic!("build.rs: cannot read fonts/ directory: {e}");
    });

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
