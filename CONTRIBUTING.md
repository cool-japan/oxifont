# Contributing to OxiFont

Thank you for your interest in contributing. OxiFont is part of the
**COOLJAPAN ecosystem** (https://github.com/cool-japan/oxifont), a
family of Pure Rust libraries that replace common C/C++/Fortran-backed
crates with FFI-free implementations. This workspace provides font
discovery, TTF/OTF/TTC/WOFF/WOFF2 parsing, TrueType hinting, glyph
subsetting, and web-font encoding across the `oxifont-core`,
`oxifont-parser`, `oxifont-hinting`, `oxifont-discovery`,
`oxifont-adapter-pure`, `oxifont-adapter-native`, `oxifont-db`,
`oxifont-subset`, `oxifont-webfont`, `oxifont-bundled`, and `oxifont`
(facade) crates.

## Building and testing

```bash
# Build the workspace
cargo build --workspace

# Run the test suite (nextest is required; do not rely on `cargo test` alone)
cargo nextest run --workspace

# Lint — this MUST produce zero warnings before a change is accepted
cargo clippy --all-targets --workspace -- -D warnings

# Format
cargo fmt --all
```

Some functionality is behind feature flags (`woff1`, `woff2`, `subset`,
`db`, `bundled-noto*`, `native`). Exercise the relevant feature
combination for any change that touches gated code, e.g.:

```bash
cargo nextest run -p oxifont-webfont --features woff1,woff2
cargo clippy -p oxifont --all-targets --features db,subset,woff1,woff2 -- -D warnings
```

## Project rules

These rules are enforced in review and, where possible, in CI:

- **No panics on untrusted font input.** `oxifont-parser`,
  `oxifont-webfont`, `oxifont-hinting`, and `oxifont-subset` all
  process byte streams that may come from an untrusted source (a font
  file downloaded from the web, embedded in a document, etc.). Do not
  add `.unwrap()`, `.expect()`, `panic!()`, `unreachable!()`, unchecked
  slice indexing, or unchecked arithmetic on data derived from such
  input outside of test code; return the crate's existing typed error
  instead (`WebFontError`, `ParserError`, `SubsetError`,
  `HintingError`, etc. — reuse the existing enum for the crate you are
  touching rather than inventing a new one). `.unwrap()`/`.expect()`
  remain fine in `#[cfg(test)]` code and in contexts that are
  genuinely infallible by construction (leave a one-line comment
  explaining why when that's the case).
- **Pure Rust by default.** No new C/C++/Fortran dependency, and no
  non-default C feature, may be added without an explicit, documented
  exception. Prefer existing COOLJAPAN replacements over `-sys`
  crates — in particular, all DEFLATE/zlib decoding goes through
  `oxiarc-deflate` and all Brotli decoding through `oxiarc-brotli`;
  never add `flate2`, `brotli`, `miniz_oxide`, or `zip`.
- **Zero clippy warnings.** `cargo clippy --all-targets -- -D
  warnings` must pass cleanly with default features (and with any
  feature combination you touch).
- **Workspace dependency inheritance.** Shared dependencies are
  declared once in the workspace `[workspace.dependencies]` table and
  pulled in via `dep.workspace = true`; do not pin ad hoc versions in
  a member crate's `Cargo.toml` when the workspace already centralizes
  that dependency.
- **File size.** Keep individual source files under 2000 lines; split
  oversized files into focused modules (the `splitrs` tool is used
  internally for this).
- **Latest crates.** Prefer the latest versions available on
  crates.io for new or updated dependencies.
- **No hardcoded absolute paths.** Tests and examples must use
  `std::env::temp_dir()` (or an equivalent relative/portable path) for
  any temporary file handling.
- **Fuzz the parsers.** `oxifont-parser`, `oxifont-webfont`,
  `oxifont-subset`, and `oxifont-db` each ship a `cargo-fuzz` harness
  under `crates/<crate>/fuzz/`. Any change to a decode/parse entry
  point should keep the corresponding target building
  (`cargo +nightly fuzz build <target>`), and new byte-oriented decode
  entry points should add one. See [SECURITY.md](SECURITY.md) for the
  full list of targets and the threat model they defend.

## Submitting changes

Open a pull request against the appropriate version branch (not
directly against a release branch, unless the project is pre-0.1.0).
Describe what changed and why, and make sure the build/test/lint
commands above all pass locally first.

For security vulnerabilities, please follow [SECURITY.md](SECURITY.md)
instead of opening a public issue or pull request.
