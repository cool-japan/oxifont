//! [`BundledFontProvider`] — registry of all fonts compiled into this crate.

/// Provides static byte slices for all bundled SIL-OFL-1.1 Noto fonts.
///
/// Entries are only present when the corresponding Cargo feature is enabled.
/// CJK entries are present but **zero-length** until the real font files are
/// placed in `crates/oxifont-bundled/fonts/cjk-*/` (see lib.rs for details).
///
/// # Example
/// ```
/// use oxifont_bundled::provider::BundledFontProvider;
///
/// let provider = BundledFontProvider::new();
/// let fonts = provider.font_data();
/// for (name, bytes) in &fonts {
///     println!("{}: {} bytes", name, bytes.len());
/// }
/// ```
#[derive(Debug, Default, Clone)]
pub struct BundledFontProvider {
    _private: (),
}

impl BundledFontProvider {
    /// Creates a new `BundledFontProvider`.
    ///
    /// No I/O is performed; all font data is available as static `&[u8]` slices
    /// that were embedded at compile time.
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Returns all bundled fonts as `(name, bytes)` pairs.
    ///
    /// The `name` is a stable identifier such as `"NotoSans-Regular"`.
    /// Bytes are a `'static` slice pointing directly into the compiled binary.
    ///
    /// CJK entries are included only when the corresponding feature is active
    /// *and* the font file was non-empty at compile time. Zero-length placeholder
    /// fonts are silently omitted from this list.
    pub fn font_data(&self) -> Vec<(&'static str, &'static [u8])> {
        #[allow(unused_mut)]
        let mut fonts: Vec<(&'static str, &'static [u8])> = Vec::new();

        #[cfg(feature = "bundled-noto")]
        {
            fonts.push(("NotoSans-Regular", crate::NOTO_SANS_REGULAR));
            fonts.push(("NotoSans-Bold", crate::NOTO_SANS_BOLD));
            fonts.push(("NotoSerif-Regular", crate::NOTO_SERIF_REGULAR));
        }

        // CJK entries: only include when the embedded bytes are non-empty
        // (zero-byte placeholders are silently skipped so callers never see
        // a corrupt "font" that fails to parse).
        #[cfg(feature = "bundled-noto-cjk-jp")]
        if !crate::NOTO_SANS_JP_REGULAR.is_empty() {
            fonts.push(("NotoSansJP-Regular", crate::NOTO_SANS_JP_REGULAR));
        }

        #[cfg(feature = "bundled-noto-cjk-kr")]
        if !crate::NOTO_SANS_KR_REGULAR.is_empty() {
            fonts.push(("NotoSansKR-Regular", crate::NOTO_SANS_KR_REGULAR));
        }

        #[cfg(feature = "bundled-noto-cjk-sc")]
        if !crate::NOTO_SANS_SC_REGULAR.is_empty() {
            fonts.push(("NotoSansSC-Regular", crate::NOTO_SANS_SC_REGULAR));
        }

        #[cfg(feature = "bundled-noto-cjk-tc")]
        if !crate::NOTO_SANS_TC_REGULAR.is_empty() {
            fonts.push(("NotoSansTC-Regular", crate::NOTO_SANS_TC_REGULAR));
        }

        fonts
    }

    /// Returns font data for a specific font by its stable name identifier.
    ///
    /// Returns `None` when the font is not bundled (feature not enabled) or
    /// when the font is a zero-byte CJK placeholder.
    ///
    /// # Example
    /// ```
    /// use oxifont_bundled::provider::BundledFontProvider;
    ///
    /// let provider = BundledFontProvider::new();
    /// if let Some(bytes) = provider.by_name("NotoSans-Regular") {
    ///     println!("NotoSans-Regular: {} bytes", bytes.len());
    /// }
    /// ```
    pub fn by_name(&self, name: &str) -> Option<&'static [u8]> {
        self.font_data()
            .into_iter()
            .find(|(n, _)| *n == name)
            .map(|(_, b)| b)
    }

    /// Returns the SIL Open Font License 1.1 text for the bundled Noto fonts.
    ///
    /// This is embedded from `../fonts/LICENSE-OFL.txt` and is always available
    /// regardless of which feature flags are active, as long as the
    /// `bundled-noto` feature is enabled.
    #[cfg(feature = "bundled-noto")]
    pub fn ofl_license_text() -> &'static str {
        include_str!("../fonts/LICENSE-OFL.txt")
    }
}
