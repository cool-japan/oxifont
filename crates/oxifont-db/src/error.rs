//! Error types for `oxifont-db`.

/// Errors produced by the font database and query engine.
///
/// This enum is `#[non_exhaustive]`: downstream `match` expressions must include
/// a catch-all arm so that new variants can be added in minor versions.
#[derive(Debug)]
#[non_exhaustive]
pub enum DbError {
    /// An I/O error occurred (file read, directory scan, etc.).
    Io(std::io::Error),
    /// A font file could not be parsed.
    ParseError(String),
    /// A cache operation failed (serialization, deserialization, or path
    /// resolution).
    Cache(String),
}

impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DbError::Io(e) => write!(f, "font-db I/O error: {e}"),
            DbError::ParseError(s) => write!(f, "font parse error: {s}"),
            DbError::Cache(s) => write!(f, "font cache error: {s}"),
        }
    }
}

impl std::error::Error for DbError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            DbError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for DbError {
    fn from(e: std::io::Error) -> Self {
        DbError::Io(e)
    }
}
