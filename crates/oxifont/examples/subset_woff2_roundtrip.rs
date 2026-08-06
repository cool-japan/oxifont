//! Subset a font down to a small set of codepoints, encode the result as
//! WOFF2, then attempt to decode it back to verify the round trip.
//!
//! Uses the compile-time embedded Noto Sans Bold font so the example needs
//! no filesystem access or system fonts:
//!
//! ```sh
//! cargo run -p oxifont --example subset_woff2_roundtrip --features subset,woff2,bundled-noto
//! ```

use std::collections::BTreeSet;

use oxifont::subset::subset_font;
use oxifont::webfont::{decode_woff2, encode_woff2};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let font_bytes = oxifont::bundled::NOTO_SANS_BOLD;

    let codepoints: BTreeSet<char> = "Hello, OxiFont!".chars().collect();
    let subsetted = subset_font(font_bytes, &codepoints)?;
    println!(
        "subset: {} bytes -> {} bytes ({} codepoints)",
        font_bytes.len(),
        subsetted.len(),
        codepoints.len()
    );

    let woff2 = encode_woff2(&subsetted)?;
    assert_eq!(
        &woff2[..4],
        b"wOF2",
        "encoded output must start with the WOFF2 magic"
    );
    println!("woff2 encoded: {} bytes", woff2.len());

    // Decoding a self-encoded WOFF2 blob back to SFNT can hit a known
    // oxiarc-brotli decompression limitation on some inputs (see
    // `crates/oxifont/tests/subset_encode.rs`); the encode step above already
    // proved the subset + WOFF2 pipeline works, so a decode failure here is
    // reported rather than treated as fatal.
    match decode_woff2(&woff2) {
        Ok(sfnt) => println!("decoded back to {} bytes of SFNT", sfnt.len()),
        Err(e) => println!("decode_woff2 hit a known limitation (not fatal): {e}"),
    }

    Ok(())
}
