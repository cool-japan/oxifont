//! Platform-specific error type for the native font adapter.

/// Platform-specific error from native font catalog operations.
///
/// Provides richer diagnostic information than the generic [`oxifont_core::FontError`].
/// Convert to [`oxifont_core::FontError`] via the [`From`] impl (or the `?` operator)
/// wherever the caller expects the generic type.
#[derive(Debug)]
pub enum NativeError {
    /// CoreText descriptor enumeration returned an unexpected failure (macOS).
    #[cfg(target_os = "macos")]
    CoreTextEnumeration(String),
    /// CoreText returned a font descriptor that could not be materialised (macOS).
    #[cfg(target_os = "macos")]
    InvalidDescriptor {
        /// Human-readable explanation of why the descriptor was rejected.
        reason: String,
    },
    /// DirectWrite COM factory creation failed (Windows).
    #[cfg(windows)]
    ComInitFailed(String),
    /// DirectWrite font collection enumeration failed (Windows).
    #[cfg(windows)]
    DWriteEnumeration(String),
    /// The native descriptor did not carry a resolvable on-disk font path.
    NoFontPath,
    /// The font file exists on disk but could not be read.
    FontReadError {
        /// The path that was attempted.
        path: std::path::PathBuf,
        /// Human-readable reason for the failure.
        reason: String,
    },
    /// The native font adapter is not supported on this platform.
    ///
    /// This variant is used when the crate is compiled on a platform for which
    /// no native font catalog backend has been implemented (e.g. Linux).
    PlatformNotSupported,
    /// An error from an underlying OxiFont subsystem.
    FontError(oxifont_core::FontError),
}

impl std::fmt::Display for NativeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            #[cfg(target_os = "macos")]
            NativeError::CoreTextEnumeration(msg) => {
                write!(f, "CoreText enumeration error: {msg}")
            }
            #[cfg(target_os = "macos")]
            NativeError::InvalidDescriptor { reason } => {
                write!(f, "invalid CoreText font descriptor: {reason}")
            }
            #[cfg(windows)]
            NativeError::ComInitFailed(msg) => {
                write!(f, "DirectWrite COM factory init failed: {msg}")
            }
            #[cfg(windows)]
            NativeError::DWriteEnumeration(msg) => {
                write!(f, "DirectWrite enumeration error: {msg}")
            }
            NativeError::NoFontPath => {
                write!(f, "no on-disk font path available from native descriptor")
            }
            NativeError::FontReadError { path, reason } => {
                write!(f, "cannot read font at {}: {reason}", path.display())
            }
            NativeError::PlatformNotSupported => {
                write!(f, "native font catalog is not supported on this platform")
            }
            NativeError::FontError(e) => write!(f, "font error: {e}"),
        }
    }
}

impl std::error::Error for NativeError {}

impl From<NativeError> for oxifont_core::FontError {
    fn from(e: NativeError) -> Self {
        oxifont_core::FontError::ParseError(e.to_string())
    }
}

impl From<oxifont_core::FontError> for NativeError {
    fn from(e: oxifont_core::FontError) -> Self {
        NativeError::FontError(e)
    }
}
