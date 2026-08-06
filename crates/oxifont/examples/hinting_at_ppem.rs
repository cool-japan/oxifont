//! Grid-fit a glyph at several pixel sizes using the TrueType bytecode
//! hinting interpreter.
//!
//! Uses the compile-time embedded Noto Sans Bold font, which carries real
//! `fpgm`/`prep`/`cvt ` tables (unlike Noto Sans Regular, which has none and
//! would only exercise the identity pass-through path):
//!
//! ```sh
//! cargo run -p oxifont --example hinting_at_ppem --features hinting,bundled-noto
//! ```

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let font_bytes = oxifont::bundled::NOTO_SANS_BOLD;
    let gid = 36u16;

    for ppem in [12u16, 16, 24, 48] {
        let outline = oxifont::hinted_outline(font_bytes, gid, ppem)?;
        println!(
            "ppem={ppem}: {} path command(s) for gid {gid}",
            outline.len()
        );
    }

    Ok(())
}
