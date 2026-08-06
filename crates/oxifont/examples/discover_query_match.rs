//! Discover installed fonts, then run a CSS-style family query against the
//! resulting catalog.
//!
//! Runs under the default feature set (`pure` + `discovery`):
//!
//! ```sh
//! cargo run -p oxifont --example discover_query_match
//! ```

use oxifont::{FontCatalog as _, FontDatabase, FontQuery};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = FontDatabase::system()?;
    println!("discovered {} font face(s)", db.faces().len());

    // Try a few common family names; not every host has all of them
    // installed (e.g. a minimal CI container may have none).
    let candidates = ["Arial", "Helvetica", "DejaVu Sans", "Liberation Sans"];
    let matched = candidates
        .iter()
        .find_map(|family| db.find(&FontQuery::new().family(*family)));

    match matched {
        Some(face) => println!(
            "matched: family={:?} weight={} style={:?} path={}",
            face.family,
            face.weight,
            face.style,
            face.path.display()
        ),
        None => println!(
            "no face matched {candidates:?} on this system \
             (expected on a fontless CI container)"
        ),
    }

    Ok(())
}
