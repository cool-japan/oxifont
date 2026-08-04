//! Parse a font's bytes, read its metrics, and extract a glyph outline.
//!
//! Uses the compile-time embedded Noto Sans Bold font so the example needs
//! no filesystem access or system fonts:
//!
//! ```sh
//! cargo run -p oxifont --example parse_metrics_outline --features bundled-noto
//! ```

use oxifont::{FontFace as _, ParsedFace};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let font_bytes = oxifont::bundled::NOTO_SANS_BOLD;
    let face = ParsedFace::parse(font_bytes, 0)?;

    println!("family: {}", face.family_name());
    println!("units/em: {}", face.units_per_em());
    println!("glyph count: {}", face.glyph_count());

    if let Some(metrics) = face.metrics() {
        println!(
            "ascender={} descender={} line_gap={}",
            metrics.ascender, metrics.descender, metrics.line_gap
        );
    }

    // Look up the glyph for 'A' and print its outline's path-command count.
    if let Some(gid) = face.glyph_for_char('A') {
        match face.outline(gid) {
            Some(outline) => println!(
                "glyph for 'A' (gid {gid}) has {} path commands",
                outline.len()
            ),
            None => println!("glyph for 'A' (gid {gid}) has no outline data"),
        }
    } else {
        println!("no glyph mapped for 'A' in this font");
    }

    Ok(())
}
