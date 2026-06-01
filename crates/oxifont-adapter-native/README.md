# oxifont-adapter-native — OS-native font adapter (CoreText / DirectWrite) for OxiFont

[![Crates.io](https://img.shields.io/crates/v/oxifont-adapter-native.svg)](https://crates.io/crates/oxifont-adapter-native)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

`oxifont-adapter-native` provides [`NativeCatalog`], a [`FontCatalog`](../oxifont-core) implementation that enumerates fonts through the operating system's own font APIs instead of scanning the filesystem. This gives access to the exact font set the OS exposes — including activated, user, and on-demand fonts — with the platform's native matching behaviour.

| Platform | Backend API |
|----------|-------------|
| macOS | CoreText (`CTFontCollection`) via `core-text` / `core-foundation` / `core-graphics` |
| Windows | DirectWrite (`IDWriteFontCollection`) via the `windows` crate (0.62) |
| other (Linux, BSD, …) | Type alias for [`oxifont_adapter_pure::FontDatabase`](../oxifont-adapter-pure) |

> **Not Pure Rust on macOS or Windows.** On those platforms this crate links the
> system font frameworks through FFI bindings (`core-text` / `core-foundation` /
> `core-graphics` on macOS; `windows` on Windows) and therefore uses `unsafe`
> code. It is the **opt-in** native backend — the default OxiFont backend is the
> Pure-Rust [`oxifont-adapter-pure`](../oxifont-adapter-pure). On any platform
> that is neither macOS nor Windows, `NativeCatalog` is simply a re-export of the
> Pure-Rust `FontDatabase`, so consumer code compiles unchanged everywhere.

## Installation

```toml
[dependencies]
oxifont-adapter-native = "0.1.0"
```

No Cargo features are required: the correct backend is selected automatically at
compile time by target `cfg`. The macOS/Windows system dependencies are pulled in
only when building for those targets.

## Quick Start

```rust,no_run
use oxifont_adapter_native::NativeCatalog;
use oxifont_core::{FontCatalog as _, FontQuery};

# fn main() -> Result<(), oxifont_adapter_native::NativeError> {
// Enumerate via CoreText (macOS) / DirectWrite (Windows) / filesystem (other).
let catalog = NativeCatalog::load()?;
println!("found {} faces", catalog.faces().len());

if let Some(face) = catalog.find(&FontQuery::new().family("Helvetica")) {
    println!("found: {}", face.family);
}
# Ok(())
# }
```

## API Overview

`NativeCatalog` implements [`oxifont_core::FontCatalog`]. The constructors and the
free functions below are available on macOS and Windows; on other platforms
`NativeCatalog` is `oxifont_adapter_pure::FontDatabase` and exposes that crate's
API instead.

### `NativeCatalog` (macOS / Windows)

| Method | Description |
|--------|-------------|
| `load() -> Result<Self, NativeError>` | Build a catalog by enumerating the OS font collection |
| `cached() -> Option<&'static NativeCatalog>` | Return a process-wide cached catalog, if one has been initialised |
| `reload(&mut self) -> Result<(), NativeError>` | Re-enumerate the OS font collection in place |
| `system() -> Result<Self, NativeError>` | Uniform cross-platform constructor (mirrors `FontDatabase::system`) |
| `system_with_options(&NativeScanOptions) -> Result<Self, NativeError>` | Construct with scan options |
| `load_face(&FaceInfo) -> Result<ParsedFace, NativeError>` | Load and parse the face described by a `FaceInfo` |
| `faces() -> &[FaceInfo]` *(trait)* | All enumerated faces |
| `find(&FontQuery) -> Option<&FaceInfo>` *(trait)* | Match a face against a query |

### `NativeScanOptions` (macOS / Windows)

Options controlling enumeration. `Default` is provided; pass to
`system_with_options`.

### Free functions (macOS only)

| Function | Description |
|----------|-------------|
| `find_font_for_codepoint(codepoint: char) -> Option<PathBuf>` | Ask CoreText which font file would render a given codepoint |
| `register_font(path: &Path) -> Result<(), FontError>` | Activate a font file with the OS for the current process |
| `unregister_font(path: &Path) -> Result<(), FontError>` | Deactivate a previously registered font file |

## Error Variants — `NativeError`

Implements `Display` and `std::error::Error`, converts to/from
[`oxifont_core::FontError`] (so `?` interoperates with the generic error type).
Several variants are platform-gated.

| Variant | Platform | Description |
|---------|----------|-------------|
| `CoreTextEnumeration(String)` | macOS | CoreText descriptor enumeration failed |
| `InvalidDescriptor { reason }` | macOS | A CoreText descriptor could not be materialised |
| `ComInitFailed(String)` | Windows | DirectWrite COM factory creation failed |
| `DWriteEnumeration(String)` | Windows | DirectWrite font-collection enumeration failed |
| `NoFontPath` | all | The native descriptor carried no resolvable on-disk path |
| `FontReadError { path, reason }` | all | The font file exists but could not be read |
| `PlatformNotSupported` | all | No native backend is implemented for this platform |
| `FontError(FontError)` | all | An error from an underlying OxiFont subsystem |

## Related Crates

- [`oxifont-adapter-pure`](../oxifont-adapter-pure) — the Pure-Rust filesystem backend; the cross-platform fallback aliased by this crate
- [`oxifont-core`](../oxifont-core) — `FontCatalog`, `FaceInfo`, `FontQuery`, `FontError`
- [`oxifont-parser`](../oxifont-parser) — used by `load_face`
- [`oxifont`](../oxifont) — facade crate; re-exports `NativeCatalog` as the `native` module and adds `system_with_native()` behind the `native` feature

## License

Apache-2.0 — COOLJAPAN OU (Team Kitasan)
