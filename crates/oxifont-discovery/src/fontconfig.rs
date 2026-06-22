//! Fontconfig XML configuration parser.
//!
//! Reads fontconfig configuration files (`/etc/fonts/fonts.conf`,
//! `~/.config/fontconfig/fonts.conf`, and XDG overrides), following
//! `<include>` directives recursively with cycle detection.
//!
//! This module requires the `fontconfig` feature.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use quick_xml::events::Event;
use quick_xml::reader::Reader;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Parse fontconfig configuration files to extract font directories.
///
/// Reads `/etc/fonts/fonts.conf` and the user fontconfig file (respecting
/// `XDG_CONFIG_HOME`), following `<include>` directives recursively.
/// Cycle detection ensures infinite loops from circular includes are prevented.
///
/// `~` and `$HOME` prefixes in `<dir>` elements are expanded to the home
/// directory. The `xdg` prefix attribute (meaning relative to
/// `$XDG_DATA_HOME`) is also supported.
///
/// Returns a deduplicated, order-preserving list of font directories.
pub fn fontconfig_font_dirs() -> Vec<PathBuf> {
    let mut visited: HashSet<PathBuf> = HashSet::new();
    let mut dirs: Vec<PathBuf> = Vec::new();

    // System config.
    parse_conf(Path::new("/etc/fonts/fonts.conf"), &mut visited, &mut dirs);

    // User config — respects XDG_CONFIG_HOME, falls back to ~/.config.
    if let Some(p) = user_fontconfig_path() {
        parse_conf(&p, &mut visited, &mut dirs);
    }

    // Deduplicate while preserving first-seen order.
    let mut seen: HashSet<PathBuf> = HashSet::new();
    dirs.retain(|d| seen.insert(d.clone()));
    dirs
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Return the user fontconfig path, honouring `XDG_CONFIG_HOME`.
fn user_fontconfig_path() -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        let mut p = PathBuf::from(xdg);
        p.push("fontconfig/fonts.conf");
        return Some(p);
    }
    let home = oxifont_core::platform_dirs::home_dir()?;
    Some(home.join(".config/fontconfig/fonts.conf"))
}

/// Expand `~` or `$HOME` prefix to the home directory.
///
/// Returns `None` if the path uses `~` / `$HOME` but the home directory
/// cannot be determined, preserving the caller's ability to skip gracefully.
fn expand_home(raw: &str) -> Option<PathBuf> {
    if let Some(rest) = raw.strip_prefix('~') {
        let home = oxifont_core::platform_dirs::home_dir()?;
        let rest = rest.trim_start_matches('/');
        return Some(if rest.is_empty() {
            home
        } else {
            home.join(rest)
        });
    }
    if let Some(rest) = raw.strip_prefix("$HOME") {
        let home = oxifont_core::platform_dirs::home_dir()?;
        let rest = rest.trim_start_matches('/');
        return Some(if rest.is_empty() {
            home
        } else {
            home.join(rest)
        });
    }
    Some(PathBuf::from(raw))
}

/// Expand a `<dir>` value, handling the `prefix` attribute.
///
/// Supported `prefix` values:
/// - (none / `"default"`) — raw or home-expanded path
/// - `"xdg"` — relative to `$XDG_DATA_HOME` (or `~/.local/share`)
fn expand_dir(raw: &str, prefix: Option<&str>) -> Option<PathBuf> {
    match prefix {
        Some("xdg") => {
            let base = if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
                PathBuf::from(xdg)
            } else {
                oxifont_core::platform_dirs::home_dir()?.join(".local/share")
            };
            let raw = raw.trim_start_matches('/');
            Some(base.join(raw))
        }
        _ => expand_home(raw),
    }
}

/// Expand an `<include>` path, handling the `prefix` attribute.
///
/// Supported `prefix` values:
/// - (none / `"default"`) — raw or home-expanded path
/// - `"xdg"` — relative to `$XDG_CONFIG_HOME` (or `~/.config`)
fn expand_include(raw: &str, prefix: Option<&str>) -> Option<PathBuf> {
    match prefix {
        Some("xdg") => {
            let base = if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
                PathBuf::from(xdg)
            } else {
                oxifont_core::platform_dirs::home_dir()?.join(".config")
            };
            let raw = raw.trim_start_matches('/');
            Some(base.join(raw))
        }
        _ => expand_home(raw),
    }
}

/// State machine for the XML parser: tracks which element we're inside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParseState {
    None,
    Dir,
    Include,
}

/// Parse a single fontconfig XML file, appending discovered `<dir>` paths to
/// `dirs` and recursively following `<include>` directives.
///
/// `visited` tracks canonicalised paths already seen to prevent cycles.
/// Missing or unreadable files are silently skipped.
pub fn parse_conf(path: &Path, visited: &mut HashSet<PathBuf>, dirs: &mut Vec<PathBuf>) {
    let content = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => return,
    };

    // Mark this path as visited using the canonical form if available.
    let key = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if !visited.insert(key) {
        return;
    }

    let mut reader = Reader::from_str(&content);
    reader.config_mut().trim_text(true);

    let mut state = ParseState::None;
    let mut current_prefix: Option<String> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                let qname = e.name();
                let name = std::str::from_utf8(qname.as_ref()).unwrap_or("");
                match name {
                    "dir" => {
                        state = ParseState::Dir;
                        current_prefix = extract_attr(e, b"prefix");
                    }
                    "include" => {
                        state = ParseState::Include;
                        current_prefix = extract_attr(e, b"prefix");
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(ref e)) => {
                if state == ParseState::None {
                    continue;
                }
                let raw = match e.decode() {
                    Ok(s) => s.trim().to_string(),
                    Err(_) => continue,
                };
                if raw.is_empty() {
                    continue;
                }
                handle_text(&raw, state, current_prefix.as_deref(), visited, dirs);
            }
            Ok(Event::CData(ref e)) => {
                if state == ParseState::None {
                    continue;
                }
                let raw = match e.decode() {
                    Ok(s) => s.trim().to_string(),
                    Err(_) => continue,
                };
                if raw.is_empty() {
                    continue;
                }
                handle_text(&raw, state, current_prefix.as_deref(), visited, dirs);
            }
            Ok(Event::End(ref e)) => {
                let qname = e.name();
                let name = std::str::from_utf8(qname.as_ref()).unwrap_or("");
                match name {
                    "dir" | "include" => {
                        state = ParseState::None;
                        current_prefix = None;
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
}

/// Process a text value captured inside a `<dir>` or `<include>` element.
fn handle_text(
    raw: &str,
    state: ParseState,
    prefix: Option<&str>,
    visited: &mut HashSet<PathBuf>,
    dirs: &mut Vec<PathBuf>,
) {
    match state {
        ParseState::Dir => {
            if let Some(expanded) = expand_dir(raw, prefix) {
                dirs.push(expanded);
            }
        }
        ParseState::Include => {
            if let Some(include_path) = expand_include(raw, prefix) {
                // An include target can be a file or a directory
                // (fontconfig supports both: dir → read all *.conf inside).
                if include_path.is_dir() {
                    parse_conf_dir(&include_path, visited, dirs);
                } else {
                    parse_conf(&include_path, visited, dirs);
                }
            }
        }
        ParseState::None => {}
    }
}

/// Parse all `*.conf` files inside a directory (fontconfig `conf.d` pattern).
///
/// Files are processed in lexicographical order, matching fontconfig's own
/// behaviour. Missing or unreadable files are silently skipped.
fn parse_conf_dir(dir: &Path, visited: &mut HashSet<PathBuf>, dirs: &mut Vec<PathBuf>) {
    let mut entries: Vec<PathBuf> = match std::fs::read_dir(dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok().map(|de| de.path()))
            .filter(|p| {
                p.extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e == "conf")
                    .unwrap_or(false)
            })
            .collect(),
        Err(_) => return,
    };
    entries.sort();
    for entry in entries {
        parse_conf(&entry, visited, dirs);
    }
}

/// Extract a named attribute value from a quick-xml `BytesStart` element.
///
/// Returns `Some(value)` if the attribute is present and valid UTF-8,
/// `None` otherwise.
fn extract_attr(e: &quick_xml::events::BytesStart<'_>, attr_name: &[u8]) -> Option<String> {
    for attr in e.attributes().flatten() {
        if attr.key.as_ref() == attr_name {
            return std::str::from_utf8(attr.value.as_ref())
                .ok()
                .map(|s| s.to_string());
        }
    }
    None
}
