/// Errors from WOFF decoding operations.
///
/// This enum is `#[non_exhaustive]`: downstream `match` expressions must include
/// a catch-all arm so that new variants can be added in minor versions.
#[derive(Debug)]
#[non_exhaustive]
pub enum WebFontError {
    /// The input data is too short to contain a valid header.
    TooShort,
    /// The magic signature is not a recognised WOFF/WOFF2 signature.
    InvalidSignature,
    /// A field value is outside the range permitted by the specification.
    InvalidField {
        /// Name of the offending field.
        field: &'static str,
        /// The actual value found.
        value: u64,
    },
    /// An offset or length in the file directory points outside the data.
    OutOfBounds {
        /// Description of the context where the out-of-bounds access occurred.
        context: &'static str,
    },
    /// A table checksum does not match the stored value.
    ChecksumMismatch {
        /// The 4-byte OpenType table tag.
        tag: [u8; 4],
    },
    /// Decompression failed (zlib or brotli).
    DecompressError(String),
    /// The decompressed table size does not match the declared origLength.
    LengthMismatch {
        /// The 4-byte OpenType table tag.
        tag: [u8; 4],
        /// Expected decompressed length.
        expected: u32,
        /// Actual decompressed length.
        got: usize,
    },
    /// The WOFF2 glyf transform sub-stream is malformed.
    MalformedGlyfTransform(String),
    /// An arithmetic overflow occurred while computing offsets or sizes.
    Overflow(&'static str),
    /// The UIntBase128 encoding is invalid (continuation without value, or overflow).
    InvalidVarInt,
    /// The font uses a feature not yet supported by this decoder.
    Unsupported(&'static str),
    /// An I/O error occurred while reading from a `Read` source.
    Io(std::io::Error),
}

impl std::fmt::Display for WebFontError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooShort => write!(f, "data too short"),
            Self::InvalidSignature => write!(f, "invalid WOFF signature"),
            Self::InvalidField { field, value } => {
                write!(f, "invalid field '{field}': value {value}")
            }
            Self::OutOfBounds { context } => write!(f, "offset out of bounds in {context}"),
            Self::ChecksumMismatch { tag } => {
                write!(
                    f,
                    "checksum mismatch for table '{}'",
                    core::str::from_utf8(tag).unwrap_or("????")
                )
            }
            Self::DecompressError(msg) => write!(f, "decompression error: {msg}"),
            Self::LengthMismatch { tag, expected, got } => {
                write!(
                    f,
                    "decompressed length mismatch for '{}': expected {expected}, got {got}",
                    core::str::from_utf8(tag).unwrap_or("????")
                )
            }
            Self::MalformedGlyfTransform(msg) => {
                write!(f, "malformed glyf transform: {msg}")
            }
            Self::Overflow(context) => write!(f, "arithmetic overflow in {context}"),
            Self::InvalidVarInt => write!(f, "invalid UIntBase128 encoding"),
            Self::Unsupported(what) => write!(f, "unsupported: {what}"),
            Self::Io(e) => write!(f, "I/O error: {e}"),
        }
    }
}

impl From<std::io::Error> for WebFontError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl std::error::Error for WebFontError {}
