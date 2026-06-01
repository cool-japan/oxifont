//! Compile-only smoke test for wasm32 target.
//!
//! This file is only compiled when targeting `wasm32` architecture.
//! Run the check with:
//!
//! ```sh
//! cargo check --target wasm32-unknown-unknown -p oxifont-bundled --features bundled-noto
//! ```
//!
//! The test itself is only reachable inside a wasm32 environment; on native
//! targets this entire file compiles to nothing.
#![cfg(target_arch = "wasm32")]

#[test]
fn wasm32_bundled_noto_data_accessible() {
    #[cfg(feature = "bundled-noto")]
    {
        let data: &[u8] = oxifont_bundled::NOTO_SANS_REGULAR;
        assert!(!data.is_empty(), "NOTO_SANS_REGULAR must not be empty");

        let italic: &[u8] = oxifont_bundled::NOTO_SANS_ITALIC;
        assert!(!italic.is_empty(), "NOTO_SANS_ITALIC must not be empty");

        let mono: &[u8] = oxifont_bundled::NOTO_SANS_MONO_REGULAR;
        assert!(!mono.is_empty(), "NOTO_SANS_MONO_REGULAR must not be empty");
    }
}
