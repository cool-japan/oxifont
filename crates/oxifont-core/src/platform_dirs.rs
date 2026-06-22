//! Pure `std`-only reimplementation of the small subset of the `dirs` crate
//! that OxiFont needs: [`home_dir`] and [`cache_dir`].
//!
//! Using these in place of the `dirs` crate keeps the entire `oxifont`
//! workspace free of the `dirs-sys` FFI dependency, satisfying the COOLJAPAN
//! Pure Rust Policy v2 L1 (no C/C++/Fortran in default features). The
//! semantics below mirror `dirs` v6 for these two functions on Linux, macOS,
//! and Windows, using only `std::env` and `std::path`.

use std::path::PathBuf;

/// Returns the path to the current user's home directory.
///
/// Pure `std` reimplementation of `dirs::home_dir` for the platforms OxiFont
/// targets.
///
/// - Unix (Linux, macOS, and other Unix): reads `$HOME`; returns `Some` when it
///   is set and non-empty, otherwise `None`.
/// - Windows: reads `%USERPROFILE%`; if empty, falls back to combining
///   `%HOMEDRIVE%` and `%HOMEPATH%`; otherwise `None`.
#[cfg(unix)]
pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// Returns the path to the current user's home directory.
///
/// Pure `std` reimplementation of `dirs::home_dir` for the platforms OxiFont
/// targets.
///
/// - Unix (Linux, macOS, and other Unix): reads `$HOME`; returns `Some` when it
///   is set and non-empty, otherwise `None`.
/// - Windows: reads `%USERPROFILE%`; if empty, falls back to combining
///   `%HOMEDRIVE%` and `%HOMEPATH%`; otherwise `None`.
#[cfg(windows)]
pub fn home_dir() -> Option<PathBuf> {
    if let Some(profile) = std::env::var_os("USERPROFILE").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(profile));
    }
    let drive = std::env::var_os("HOMEDRIVE").filter(|v| !v.is_empty());
    let path = std::env::var_os("HOMEPATH").filter(|v| !v.is_empty());
    match (drive, path) {
        (Some(drive), Some(path)) => {
            let mut combined = PathBuf::from(drive);
            combined.push(PathBuf::from(path));
            Some(combined)
        }
        _ => None,
    }
}

/// Returns the path to the current user's cache directory.
///
/// Pure `std` reimplementation of `dirs::cache_dir` for the platforms OxiFont
/// targets.
///
/// - Linux (and other non-macOS Unix): `$XDG_CACHE_HOME` when set and
///   non-empty, otherwise `home_dir()?.join(".cache")`.
/// - macOS: `home_dir()?.join("Library/Caches")`.
/// - Windows: `%LOCALAPPDATA%` when set and non-empty, otherwise `None`.
#[cfg(all(unix, not(target_os = "macos")))]
pub fn cache_dir() -> Option<PathBuf> {
    match std::env::var_os("XDG_CACHE_HOME").filter(|v| !v.is_empty()) {
        Some(xdg) => Some(PathBuf::from(xdg)),
        None => home_dir().map(|home| home.join(".cache")),
    }
}

/// Returns the path to the current user's cache directory.
///
/// Pure `std` reimplementation of `dirs::cache_dir` for the platforms OxiFont
/// targets.
///
/// - Linux (and other non-macOS Unix): `$XDG_CACHE_HOME` when set and
///   non-empty, otherwise `home_dir()?.join(".cache")`.
/// - macOS: `home_dir()?.join("Library/Caches")`.
/// - Windows: `%LOCALAPPDATA%` when set and non-empty, otherwise `None`.
#[cfg(target_os = "macos")]
pub fn cache_dir() -> Option<PathBuf> {
    home_dir().map(|home| home.join("Library/Caches"))
}

/// Returns the path to the current user's cache directory.
///
/// Pure `std` reimplementation of `dirs::cache_dir` for the platforms OxiFont
/// targets.
///
/// - Linux (and other non-macOS Unix): `$XDG_CACHE_HOME` when set and
///   non-empty, otherwise `home_dir()?.join(".cache")`.
/// - macOS: `home_dir()?.join("Library/Caches")`.
/// - Windows: `%LOCALAPPDATA%` when set and non-empty, otherwise `None`.
#[cfg(windows)]
pub fn cache_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_dir_returns_some_or_none_without_panicking() {
        // Must never panic regardless of environment.
        let _ = home_dir();
    }

    #[test]
    fn cache_dir_returns_some_or_none_without_panicking() {
        let _ = cache_dir();
    }

    #[cfg(unix)]
    #[test]
    fn home_dir_reads_home_env() {
        // On Unix CI/hosts $HOME is typically set; when set it must be returned.
        if let Some(expected) = std::env::var_os("HOME").filter(|v| !v.is_empty()) {
            assert_eq!(home_dir(), Some(PathBuf::from(expected)));
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn cache_dir_falls_back_to_dot_cache() {
        // When XDG_CACHE_HOME is unset, cache_dir should end with `.cache`
        // (provided a home directory exists).
        if std::env::var_os("XDG_CACHE_HOME").is_none() {
            if let Some(dir) = cache_dir() {
                assert!(dir.ends_with(".cache"));
            }
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn cache_dir_uses_library_caches() {
        if let Some(dir) = cache_dir() {
            assert!(dir.ends_with("Library/Caches"));
        }
    }
}
