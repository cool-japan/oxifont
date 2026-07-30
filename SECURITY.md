# Security Policy

## Supported versions

OxiFont follows the COOLJAPAN ecosystem's rolling-release model. Only
the **latest released 0.x line** is supported with security fixes.
Older 0.x releases do not receive backported patches; please upgrade
to the latest release before reporting an issue to confirm it is
still reproducible.

## Reporting a vulnerability

Please **do not** file a public GitHub issue for a suspected security
vulnerability. Instead, report it privately by emailing:

**info@kitasan.io**

Include as much detail as you can: affected crate and version, a
minimal reproduction (a font file or byte sequence that triggers the
issue is ideal), the observed impact (panic, hang, excessive memory,
incorrect output, etc.), and — if known — a suggested fix or
mitigation.

Reports are triaged privately by the maintainer. We will acknowledge
receipt as soon as practical, investigate, and coordinate a fix and
disclosure timeline with the reporter before any public disclosure.

## Threat model

OxiFont's parsers (`oxifont-parser`, `oxifont-webfont`, and the
`SfntTableMap`/hinting code in `oxifont-core`/`oxifont-hinting`) are
designed to safely consume **untrusted, potentially adversarial**
TTF/OTF/TTC/WOFF/WOFF2 byte streams — e.g. fonts embedded in a
document, downloaded from the web, or supplied by an untrusted user.
Malformed or hostile input must never cause:

- a panic (`unwrap()`, `expect()`, `panic!()`, `unreachable!()`,
  out-of-bounds indexing, or integer-overflow abort in debug builds),
- unbounded memory allocation (e.g. a crafted small file that claims a
  multi-gigabyte glyph count),
- an infinite loop or unbounded CPU hang,
- or memory unsafety.

Malformed input should instead produce a typed error from the crate's
existing error enum (e.g. `WebFontError`, `ParserError`,
`SubsetError`, `HintingError`). This property is exercised by the
project's `cargo-fuzz` harnesses — see below — and is a hard
requirement for any change that touches a decode/parse path.

`oxifont-discovery` and `oxifont-adapter-pure`/`oxifont-adapter-native`
enumerate and read font files from the local filesystem or OS font
APIs (fontconfig-equivalent scan, CoreText, DirectWrite); they trust
the *paths* the OS reports but still treat the *bytes* of every font
file found there as untrusted input subject to the same threat model.

## Fuzzing

Four `cargo-fuzz` harnesses exercise the untrusted-input surface and
are expected to build and run cleanly on every change to a decode
path:

| Crate | Fuzz targets |
|---|---|
| `oxifont-parser` | `fuzz_parse`, `fuzz_face_methods` |
| `oxifont-webfont` | `fuzz_woff1_decode`, `fuzz_woff2_decode`, `fuzz_detect_auto` |
| `oxifont-subset` | `fuzz_subset`, `fuzz_subset_by_gids` |
| `oxifont-db` | `fuzz_query` |

Run e.g. `cargo +nightly fuzz run fuzz_woff2_decode` from
`crates/oxifont-webfont/` (or the corresponding crate directory) to
fuzz locally. New byte-oriented decode/parse entry points should add a
corresponding target.

## Scope

This policy covers the OxiFont crates published from this repository
(https://github.com/cool-japan/oxifont): `oxifont-core`,
`oxifont-parser`, `oxifont-hinting`, `oxifont-discovery`,
`oxifont-adapter-pure`, `oxifont-adapter-native`, `oxifont-db`,
`oxifont-webfont`, `oxifont-subset`, `oxifont-bundled`, and the
`oxifont` facade crate. Vulnerabilities in upstream dependencies
(`ttf-parser`, `oxiarc-deflate`, `oxiarc-brotli`, etc.) should be
reported to those projects directly, though we welcome a heads-up so
we can track and update our dependency pins.

## Maintainer

COOLJAPAN OU (Team Kitasan)
