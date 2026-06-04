//! Fuzz target: feed arbitrary bytes into `ParsedFace::parse` then call
//! deeper methods — outline, kern, variation axes, feature tags — that
//! exercise more of the OpenType table parsing paths.

#![no_main]

use libfuzzer_sys::fuzz_target;
use oxifont_core::FontFace as _;

fuzz_target!(|data: &[u8]| {
    let Ok(face) = oxifont_parser::ParsedFace::parse(data.to_vec(), 0) else {
        return;
    };

    let gc = face.glyph_count();

    // Exercise outline extraction on a few GIDs that may exist.
    for gid in [0u16, 1, 2, 10, 50] {
        if gid < gc {
            let _ = face.outline(gid);
        }
    }

    // Exercise kern table for a few pairs.
    for left in [0u16, 1] {
        for right in [0u16, 1] {
            if left < gc && right < gc {
                let _ = face.kern(left, right);
            }
        }
    }

    // Variable font axes.
    let _ = face.axes();
    let _ = face.is_variable();

    // GSUB/GPOS feature tags.
    let _ = face.gsub_feature_tags();
    let _ = face.gpos_feature_tags();
    let _ = face.supported_scripts();

    // Additional metrics.
    let _ = face.metrics();
    let _ = face.color_glyph_format();
    let _ = face.postscript_name();

    // Vertical metrics.
    if gc > 0 {
        let _ = face.vertical_advance(0);
        let _ = face.vertical_origin(0);
    }
});
