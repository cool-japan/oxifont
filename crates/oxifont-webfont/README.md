# oxifont-webfont — Pure-Rust WOFF1/WOFF2 codec for OxiFont

[![Crates.io](https://img.shields.io/crates/v/oxifont-webfont.svg)](https://crates.io/crates/oxifont-webfont)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

`oxifont-webfont` encodes and decodes web-font containers — **WOFF1** (zlib per-table compression) and **WOFF2** (single brotli stream with the glyf/loca table transform) — to and from raw SFNT (TrueType/OpenType) byte buffers. Decoded SFNT output can be handed directly to [`oxifont-parser`](../oxifont-parser); encoded output is a standards-compliant `.woff` / `.woff2` file.

The crate is **100% Pure Rust**: compression is provided by the COOLJAPAN [`oxiarc-deflate`](https://crates.io/crates/oxiarc-deflate) (zlib/DEFLATE) and [`oxiarc-brotli`](https://crates.io/crates/oxiarc-brotli) crates — no C `zlib`, `brotli`, or `woff2` libraries are linked. The codec forbids `unsafe` code (`#![forbid(unsafe_code)]`).

## Installation

```toml
[dependencies]
# WOFF2 only (brotli)
oxifont-webfont = { version = "0.1.0", features = ["woff2"] }

# WOFF1 only (zlib)
oxifont-webfont = { version = "0.1.0", features = ["woff1"] }

# Both
oxifont-webfont = { version = "0.1.0", features = ["woff1", "woff2"] }
```

Without any feature flag the crate still compiles and exposes the format-detection
and SFNT-assembly helpers, but the `decode_*` / `encode_*` functions for a given
format require its feature.

## Quick Start

```rust,no_run
# #[cfg(feature = "woff2")]
# fn main() -> Result<(), oxifont_webfont::WebFontError> {
// Decode a WOFF2 file into an SFNT byte buffer, then parse it.
let woff2 = std::fs::read("font.woff2")?;
let sfnt = oxifont_webfont::decode_woff2(&woff2)?;
let face = oxifont_parser::ParsedFace::parse(sfnt, 0)
    .expect("decoded SFNT should parse");
println!("{}", oxifont_core::FontFace::family_name(&face));

// Round-trip: re-encode the SFNT back to WOFF2.
let reencoded = oxifont_webfont::encode_woff2(&oxifont_webfont::decode_woff2(&woff2)?)?;
assert_eq!(&reencoded[0..4], b"wOF2");
# Ok(())
# }
# #[cfg(not(feature = "woff2"))]
# fn main() {}
```

### Format-agnostic decoding

`decode_auto` inspects the magic bytes and dispatches to the right decoder
(passing SFNT through unchanged), returning the SFNT plus any embedded WOFF1
extended-metadata XML:

```rust,no_run
# fn main() -> Result<(), oxifont_webfont::WebFontError> {
let data = std::fs::read("font.woff2")?;
let result = oxifont_webfont::decode_auto(&data)?;
println!("{} SFNT bytes", result.sfnt.len());
if let Some(meta) = result.metadata {
    println!("metadata: {meta}");
}
# Ok(())
# }
```

### Streaming WOFF2

`decode_woff2_streaming` decodes from any `impl Read` without loading the whole
file into memory — the brotli-compressed data block is streamed through the
decompressor:

```rust,no_run
# #[cfg(feature = "woff2")]
# fn main() -> Result<(), oxifont_webfont::WebFontError> {
let file = std::fs::File::open("large.woff2")?;
let sfnt = oxifont_webfont::decode_woff2_streaming(file)?;
println!("{} SFNT bytes", sfnt.len());
# Ok(())
# }
# #[cfg(not(feature = "woff2"))]
# fn main() {}
```

## API Overview

### Top-level functions

| Function | Feature | Description |
|----------|---------|-------------|
| `decode_woff1(data) -> Result<Vec<u8>, WebFontError>` | `woff1` | Decode a WOFF1 file into an SFNT byte buffer |
| `encode_woff1(sfnt_data) -> Result<Vec<u8>, WebFontError>` | `woff1` | Encode an SFNT buffer as WOFF1 (zlib level 9; tables stored uncompressed if compression does not shrink them) |
| `decode_woff2(data) -> Result<Vec<u8>, WebFontError>` | `woff2` | Decode a single-font WOFF2 file into an SFNT byte buffer |
| `encode_woff2(sfnt_data) -> Result<Vec<u8>, WebFontError>` | `woff2` | Encode an SFNT buffer as WOFF2 (applies the glyf/loca forward transform for TrueType fonts) |
| `decode_woff2_streaming<R: Read>(reader) -> Result<Vec<u8>, WebFontError>` | `woff2` | Decode WOFF2 from any `Read` source without buffering the whole file |
| `decode_woff2_collection(data) -> Result<Vec<Vec<u8>>, WebFontError>` | `woff2` | Decode every font from a WOFF2 collection (`ttcf` flavor) into one SFNT buffer each |
| `extract_woff2_private_data(data) -> Option<Vec<u8>>` | `woff2` | Extract the WOFF2 private-data block, or `None` if absent / out of range |

### `detect` module

| Item | Description |
|------|-------------|
| `detect_format(data) -> FontFormat` | Identify the container from the first 4 magic bytes |
| `decode_auto(data) -> Result<DecodeResult, WebFontError>` | Detect and decode any supported format into SFNT (SFNT input is passed through); feature-gated decoders |
| `FontFormat` | Enum: `Sfnt`, `Woff1`, `Woff2`, `Unknown` |
| `DecodeResult` | Struct: `sfnt: Vec<u8>`, `metadata: Option<String>` (WOFF1 metadata XML) |

`detect_format`, `decode_auto`, `DecodeResult`, and `FontFormat` are also re-exported at the crate root.

### `sfnt` module — SFNT assembly helpers

| Item | Description |
|------|-------------|
| `build_sfnt(sfnt_version, tables) -> Result<Vec<u8>, WebFontError>` | Assemble an SFNT from `(tag, data)` pairs: offset table, table directory, padded data, and a corrected `head.checkSumAdjustment` |
| `detect_sfnt_version(tables) -> u32` | Return `SFNT_MAGIC_CFF` if a `CFF `/`CFF2` table is present, else `SFNT_MAGIC_TT` |
| `table_checksum(data) -> u32` | OpenType/TrueType per-table checksum (sum of big-endian uint32 words) |
| `pad4(data: &mut Vec<u8>)` | Zero-pad a buffer in place to a 4-byte boundary |
| `SFNT_MAGIC_TT: u32` | TrueType outline magic (`0x00010000`) |
| `SFNT_MAGIC_CFF: u32` | OpenType/CFF outline magic (`OTTO`) |

### `woff1` / `woff2` modules

Feature-gated low-level decoder/encoder implementations. The stable entry points
are the top-level `decode_*` / `encode_*` functions above; these modules are
public for advanced use.

## Feature Flags

| Feature | Default | Pulls in | Enables |
|---------|---------|----------|---------|
| `woff1` | no | `oxiarc-deflate` | `decode_woff1`, `encode_woff1`, and the `woff1` module |
| `woff2` | no | `oxiarc-brotli` | `decode_woff2`, `encode_woff2`, `decode_woff2_streaming`, `decode_woff2_collection`, `extract_woff2_private_data`, and the `woff2` module |

## Error Variants — `WebFontError`

Implements `Display`, `std::error::Error`, and `From<std::io::Error>`.

| Variant | Description |
|---------|-------------|
| `TooShort` | Input is too short to contain a valid header |
| `InvalidSignature` | Magic bytes are not a recognised WOFF/WOFF2 signature |
| `InvalidField { field, value }` | A field value is outside the range permitted by the spec |
| `OutOfBounds { context }` | A directory offset/length points outside the data |
| `ChecksumMismatch { tag }` | A table checksum does not match the stored value |
| `DecompressError(String)` | zlib or brotli decompression failed |
| `LengthMismatch { tag, expected, got }` | Decompressed table size differs from the declared `origLength` |
| `MalformedGlyfTransform(String)` | The WOFF2 glyf-transform sub-stream is malformed |
| `Overflow(&'static str)` | Arithmetic overflow while computing offsets/sizes |
| `InvalidVarInt` | Invalid UIntBase128 encoding |
| `Unsupported(&'static str)` | A feature (e.g. a missing decoder feature flag) is not available |
| `Io(std::io::Error)` | I/O error reading from a `Read` source |

## Related Crates

- [`oxifont-parser`](../oxifont-parser) — parse the decoded SFNT into a `ParsedFace`
- [`oxifont-subset`](../oxifont-subset) — reduce a font to a glyph subset before WOFF2 encoding
- [`oxifont-core`](../oxifont-core) — `FontFace` / `FontError` and shared types
- [`oxifont`](../oxifont) — facade crate; re-exports this codec as the `webfont` module behind the `woff1` / `woff2` features

## License

Apache-2.0 — COOLJAPAN OU (Team Kitasan)
