# oxifont-webfont TODO

## Status
Pure Rust WOFF1 and WOFF2 decoder. WOFF1: zlib decompression via `oxiarc-deflate`, header/directory parsing, checksum verification. WOFF2: brotli decompression via `oxiarc-brotli`, transformed glyf/loca/hmtx reconstruction (Felzenszwalb triplet decoding, 255UInt16, composite glyph handling, bbox bitmap, instruction streams), SFNT assembly with checkSumAdjustment. 7 source files, ~1077 SLOC across woff1.rs, woff2/mod.rs, woff2/header.rs, woff2/glyf.rs, woff2/hmtx.rs, sfnt.rs, error.rs. Decoders are feature-gated. Missing WOFF encoding and collection support.

## Core Implementation
- [x] Implement WOFF1 encoder: compress SFNT tables with zlib, write WOFF1 header/directory (~150 SLOC) (planned 2026-05-25)
  - **Goal:** Given valid SFNT bytes, emit spec-conformant WOFF1 that the existing decoder round-trips back to byte-identical SFNT.
  - **Design:** New `src/woff1/encode.rs`. Reuse `sfnt::detect_sfnt_version`, `sfnt::table_checksum`, `sfnt::pad4`. Per-table `oxiarc_deflate::zlib_compress(table, 9)`, keep compressed only if smaller (else store raw, compLength==origLength). Write WOFF1 header (`wOFF` 0x774F4646, flavor=sfnt-version, numTables, totalSfntSize, reserved=0, meta/priv=0), table directory (tag/offset/compLength/origLength/origChecksum), 4-byte-aligned compressed blocks.
  - **Files:** `crates/oxifont-webfont/src/woff1/encode.rs` (new), `crates/oxifont-webfont/src/woff1.rs` or `woff1/mod.rs` (`mod encode;`), `crates/oxifont-webfont/src/lib.rs` (export `encode_woff1`).
  - **Prerequisites:** none (oxiarc-deflate already a dependency).
  - **Tests:** `tests/woff1_encode.rs` — round-trip with `build_sfnt`-constructed SFNT and real TTF; assert decode(encode(x)) tables==x; exercise store vs compress branch.
  - **Risk:** checksum/totalSfntSize miscalculation → decoder rejects. Mitigation: reuse sfnt helpers; round-trip catches it.
- [x] Implement WOFF2 encoder: apply glyf/loca/hmtx transforms, brotli compress, write WOFF2 header/directory (~300 SLOC) (planned 2026-05-25)
  - **Goal:** Emit spec-conformant WOFF2 that the existing WOFF2 decoder round-trips to equivalent SFNT, exercising both transformed and null-transform paths.
  - **Design:** New `src/woff2/encode.rs` (separate testable layer). Inverse of decoder in `woff2/glyf.rs` + `woff2/hmtx.rs`. glyf/loca forward transform: emit transformed glyf (numGlyphs, indexFormat, stream offsets, bbox bitmap, triplet-encoded point deltas). hmtx: null transform initially (version byte=1 for null). Transform-version CRITICAL asymmetry: glyf/loca bits6-7=3=null/0=transformed; hmtx 0=transformed/1=null; all others 0=null. Single brotli stream via `oxiarc_brotli::compress(stream, 11)`. UIntBase128 + 255UInt16 writers (inverse of decoder readers). WOFF2 header: `wOF2` 0x774F4632.
  - **Files:** `crates/oxifont-webfont/src/woff2/encode.rs` (new), submodule `woff2/encode/glyf.rs` + `woff2/encode/varint.rs` if size demands, `woff2/mod.rs` (`mod encode;`), `src/lib.rs` (export `encode_woff2`).
  - **Prerequisites:** none (oxiarc-brotli already a dependency).
  - **Tests:** `tests/woff2_encode.rs` — round-trip real TTF + build_sfnt minimal SFNT; unit tests on transform layer; varint write↔read identity (0, 127, 128, 2^28-1); assert decoded tables match original.
  - **Risk:** triplet encoding off-by-one corrupts all points. Mitigation: transform layer unit-tested independently; explicit per-tag transform-version handling. woff2/glyf.rs already 1076 lines — encoder in own files.
- [x] Support WOFF2 font collections (TTC-in-WOFF2): parse CollectionHeader, handle shared tables (~80 SLOC)
- [x] Support WOFF1 metadata block parsing (XML metadata for font licensing info) (~40 SLOC)
- [x] Support WOFF2 metadata block parsing (~40 SLOC)
- [x] Add WOFF2 private data block handling (~15 SLOC)
- [x] Fix triplet decode accuracy: verify coordinate deltas against reference WOFF2 decoder for all 128 flag byte encodings (~50 SLOC)
- [x] Handle WOFF2 hmtx transform option flags correctly: bit 0 = proportional lsbs from glyf xMin, bit 1 = monospace lsbs from glyf xMin (~20 SLOC)
- [x] Add CFF/CFF2 support in WOFF2: detect CFF outlines and skip glyf transform (CFF tables are not transformed in WOFF2) (~15 SLOC)
- [x] Implement SFNT-to-WOFF1 conversion pipeline: `encode_woff1(sfnt_data) -> Vec<u8>` (~30 SLOC) (planned 2026-05-25)
  - **Goal:** Public entry point calling the WOFF1 encoder; exported from lib.rs.
  - **Design:** Thin wrapper in `src/lib.rs` or `src/woff1/mod.rs` that calls the encoder impl and maps errors.
  - **Files:** `crates/oxifont-webfont/src/lib.rs`.
  - **Prerequisites:** WOFF1 encoder (item line 7 above).
  - **Tests:** Covered by `tests/woff1_encode.rs` round-trip.
  - **Risk:** None beyond the encoder itself.
- [x] Implement SFNT-to-WOFF2 conversion pipeline: `encode_woff2(sfnt_data) -> Vec<u8>` (~30 SLOC) (planned 2026-05-25)
  - **Goal:** Public entry point calling the WOFF2 encoder; exported from lib.rs.
  - **Design:** Thin wrapper in `src/lib.rs` or `src/woff2/mod.rs` calling the encoder impl and mapping errors.
  - **Files:** `crates/oxifont-webfont/src/lib.rs`.
  - **Prerequisites:** WOFF2 encoder (item line 8 above).
  - **Tests:** Covered by `tests/woff2_encode.rs` round-trip.
  - **Risk:** None beyond the encoder itself.
- [x] Add TTF-to-WOFF2 conversion with subsetting: `subset_and_encode_woff2(font_data, codepoints) -> Vec<u8>` (~20 SLOC) (planned 2026-05-25)
  - **Goal:** One-call subset+encode-to-WOFF2; implemented in the facade `oxifont` crate (not here) to avoid a webfont↔subset dependency cycle.
  - **Design:** Lives in `crates/oxifont/src/lib.rs` behind `subset`+`woff2` feature combo. Body: `oxifont_subset::subset_font(data, codepoints)?` → `oxifont_webfont::encode_woff2(&sfnt)?`.
  - **Files:** `crates/oxifont/src/lib.rs`, `crates/oxifont/Cargo.toml` (feature wiring, no version bump).
  - **Prerequisites:** encode_woff2 (line 17 above).
  - **Tests:** `crates/oxifont/tests/subset_encode.rs` feature-gated under `#[cfg(all(feature="subset",feature="woff2"))]`.
  - **Risk:** feature-flag matrix. Mitigation: explicit #[cfg]; test under combined feature; verify default-feature build.

## API Improvements
- [x] Add `detect_format(data: &[u8]) -> FontFormat` for auto-detecting TTF/OTF/WOFF1/WOFF2 from magic bytes (~20 SLOC) (planned 2026-05-25)
  - **Goal:** Sniff first 4 bytes: 0x774F4646→Woff1, 0x774F4632→Woff2, 0x00010000/OTTO/true/ttcf→Sfnt, else Unknown.
  - **Design:** `FontFormat` enum {Sfnt, Woff1, Woff2, Unknown}. Length-guard before reads (never panics on short input).
  - **Files:** `crates/oxifont-webfont/src/detect.rs` (new, ~40 lines), `src/lib.rs` (export FontFormat + detect_format).
  - **Prerequisites:** none.
  - **Tests:** `tests/detect.rs` — detect on each magic + truncated/empty → Unknown; never panics.
  - **Risk:** ambiguous/short headers. Mitigation: explicit length guard + Unknown catch-all.
- [x] Add `decode_auto(data: &[u8]) -> Result<Vec<u8>>` that detects format and decodes accordingly (~15 SLOC) (planned 2026-05-25)
  - **Goal:** Single entry point → dispatches to woff1/woff2/passthrough decoder; returns DecodeResult.
  - **Design:** Calls detect_format, dispatches decode_woff1/decode_woff2/passthrough. Signature upgraded to return DecodeResult once that struct exists.
  - **Files:** `crates/oxifont-webfont/src/detect.rs`, `src/lib.rs`.
  - **Prerequisites:** detect_format (line 21).
  - **Tests:** Covered by `tests/detect.rs` — decode_auto on encode_woff1/encode_woff2 output and raw SFNT all yield equivalent SFNT.
  - **Risk:** None.
- [x] Return metadata alongside decoded SFNT: `DecodeResult { sfnt: Vec<u8>, metadata: Option<String> }` (planned 2026-05-25)
  - **Goal:** Structured return type from decode_auto carrying SFNT bytes + optional metadata string (WOFF1 extended-metadata XML if present).
  - **Design:** Pub struct in detect.rs. decode_auto returns Result<DecodeResult>. SFNT passthrough: metadata=None.
  - **Files:** `crates/oxifont-webfont/src/detect.rs`, `src/lib.rs`.
  - **Prerequisites:** detect_format (line 21).
  - **Tests:** Covered by detect.rs tests.
  - **Risk:** None.
- [x] Add streaming decode: `decode_woff2_streaming(reader: impl Read) -> Result<Vec<u8>>` for large files (planned 2026-05-27)
  - **Goal:** Public entry point `decode_woff2_streaming<R: Read>(reader: R) -> Result<Vec<u8>, WebFontError>` that decodes a WOFF2 stream without loading the entire compressed payload into memory first. Round-trips identically to `decode_woff2(&[u8])` on encoded fixtures.
  - **Design:** New `src/woff2/streaming.rs`. Strategy: (a) read the 48-byte WOFF2 header into a `[u8; 48]`, (b) parse `numTables` (offset 12, u16), read table-directory entries iteratively via a small `BufRead` adapter, (c) use `oxiarc_brotli::streaming::BrotliDecompressor::new(reader.take(totalCompressedSize))` to obtain a decompressing reader, (d) read decompressed table data chunk-by-chunk into an assembled SFNT buffer (avoids compressed-payload full allocation), (e) run existing `woff2::glyf::reconstruct` and `woff2::hmtx::reconstruct` on per-table slices, assemble via existing `sfnt` helpers. `WebFontError::Io(io::Error)` variant added if missing.
  - **Files:** `src/woff2/streaming.rs` (new, ~250 lines), `src/woff2/mod.rs` (export), `src/lib.rs` (export `decode_woff2_streaming`), `src/error.rs` (add `Io` variant if absent).
  - **Prerequisites:** `oxiarc-brotli` `streaming::BrotliDecompressor<R: Read>` already public at `~/work/oxiarc/oxiarc-brotli/src/streaming.rs` L242.
  - **Tests:** `tests/woff2_streaming.rs` — `streaming_decode_matches_one_shot`, `streaming_decode_assembled_sfnt_parses`, `streaming_decode_truncated_reader_errors`.
  - **Streaming bench:** `benches/woff2_streaming.rs` (new) — criterion bench one-shot vs streaming. Requires `criterion.workspace = true` in `Cargo.toml` (Slice 5 adds the workspace dep first).
  - **Risk:** Chunk boundaries may not align with table boundaries. Mitigation: intermediate decompressed `Vec<u8>` sized by sum of `origLength` values, then slice per-table.

## Testing
- [x] Add real WOFF1 font fixture and test full decode round-trip (decode then parse with ttf-parser)
- [x] Add real WOFF2 font fixture and test full decode round-trip
- [x] Test WOFF2 transformed glyf with simple and composite glyphs
- [x] Test WOFF2 hmtx reconstruction with and without lsb omission flags
- [x] Test WOFF2 UIntBase128 edge cases: maximum value, overflow, leading zeros
- [x] Test checksum verification: modify a byte in a decoded table and verify detection
- [x] Test WOFF1 uncompressed tables (comp_length == orig_length)
- [ ] Test WOFF2 collection decoding once implemented
- [ ] Fuzz WOFF1 and WOFF2 decoders with arbitrary bytes
- [x] Benchmark decode time for Google Fonts WOFF2 files (Roboto, Noto Sans) (planned 2026-05-26)
  - **Design:** `benches/woff2_decode.rs` — encode test.ttf fixture once, bench `decode_woff2(&bytes)` and `decode_woff2_streaming(Cursor::new(&bytes))`. Requires `criterion.workspace = true` in `[dev-dependencies]` + `[[bench]] name = "woff2_decode" harness = false`. Workspace criterion dep added by Slice 5 first.

## Performance
- [x] Pre-allocate decompressed buffers using origLength hints from table directory
  - WOFF1 uncompressed path: pre-allocates with `Vec::with_capacity(orig_length)` before `extend_from_slice`.
  - WOFF1/WOFF2 compressed paths: blocked on upstream oxiarc-deflate/oxiarc-brotli not exposing a `decompress_with_capacity(data, hint)` API; documented in code.
- [ ] Avoid intermediate Vec allocations during table transform reconstruction
- [x] Stream brotli decompression directly into table slicing (subsumed by streaming decoder above — will be marked done as part of that implementation)
- [ ] Benchmark WOFF2 decode against woff2-rs and fontkit for performance comparison

## Integration
- [x] Pipeline with oxifont-subset: subset first, then encode to WOFF2 for web delivery — `oxifont::subset_and_encode_woff2()` in facade crate
- [x] Provide decoded SFNT to oxifont-parser for immediate face parsing after decode — `oxifont::decode_and_parse()` in facade crate
- [x] Integrate with oxifont facade crate's `webfont` feature module — `pub mod webfont { pub use oxifont_webfont::*; }` in oxifont/src/lib.rs
