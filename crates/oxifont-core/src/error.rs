//! Error type for OxiFont crates.

extern crate alloc;

#[cfg(feature = "std")]
use std::sync::Arc;

/// Errors returned by OxiFont crates.
///
/// `FontError` implements `Clone`: the `IoError` variant (std only) wraps the
/// underlying [`std::io::Error`] in an [`Arc`] so that it can be cheaply
/// cloned without copying OS error state.
///
/// # Example
/// ```
/// use oxifont_core::FontError;
/// let err = FontError::ParseError("bad magic bytes".to_string());
/// assert_eq!(err.to_string(), "font parse error: bad magic bytes");
///
/// let not_found = FontError::NotFound;
/// assert_eq!(not_found.to_string(), "font not found");
///
/// let oob = FontError::IndexOutOfBounds { index: 3, count: 2 };
/// assert_eq!(oob.to_string(), "face index 3 out of bounds (count=2)");
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum FontError {
    /// The font bytes could not be parsed.
    ParseError(alloc::string::String),
    /// An I/O error occurred while reading a font file.
    #[cfg(feature = "std")]
    #[cfg_attr(feature = "serde", serde(serialize_with = "serialize_io_error"))]
    IoError(Arc<std::io::Error>),
    /// No matching face was found.
    NotFound,
    /// The format is not TTF, OTF, or TTC.
    UnsupportedFormat,
    /// The requested face index is beyond the collection size.
    IndexOutOfBounds {
        /// The requested index.
        index: u32,
        /// The number of faces in the collection.
        count: u32,
    },
}

#[cfg(all(feature = "serde", feature = "std"))]
fn serialize_io_error<S>(err: &Arc<std::io::Error>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(&err.to_string())
}

impl core::fmt::Display for FontError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            FontError::ParseError(s) => write!(f, "font parse error: {s}"),
            #[cfg(feature = "std")]
            FontError::IoError(e) => write!(f, "font I/O error: {e}"),
            FontError::NotFound => write!(f, "font not found"),
            FontError::UnsupportedFormat => write!(f, "unsupported font format"),
            FontError::IndexOutOfBounds { index, count } => {
                write!(f, "face index {index} out of bounds (count={count})")
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for FontError {}

#[cfg(feature = "std")]
impl From<std::io::Error> for FontError {
    fn from(e: std::io::Error) -> Self {
        FontError::IoError(Arc::new(e))
    }
}
