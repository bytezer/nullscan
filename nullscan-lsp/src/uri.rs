//! Minimal `file:` URI conversion for LSP document URIs.
//!
//! `lsp-types` 0.97 wraps `fluent-uri`, which has no `to_file_path`/`from_file_path`
//! pair. Both directions the server needs live here: incoming document URIs to real
//! paths (percent-decoding plus drive/UNC handling on Windows) and local paths back to
//! URIs for `publishDiagnostics`.

use std::path::{Path, PathBuf};

use lsp_types::Uri;

/// Returns the local path behind a `file:` URI, or `None` for other schemes.
pub fn to_file_path(uri: &Uri) -> Option<PathBuf> {
    if !uri.scheme()?.eq_lowercase("file") {
        return None;
    }
    let path = uri.path().as_estr().decode().into_string().ok()?;
    let host = uri
        .authority()
        .map(|authority| authority.as_str())
        .unwrap_or("");
    Some(file_path(host, path.as_ref()))
}

/// Builds the `file:` URI for a local path, percent-encoding everything that is not a
/// safe path byte: spaces, `#`, `?`, `%` and every non-ASCII character such as `ñ`.
///
/// A relative path is made absolute first, because LSP URIs must be absolute.
pub fn from_file_path(path: &Path) -> Uri {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let text = absolute.to_string_lossy();
    let uri = if cfg!(windows) {
        windows_uri(&text)
    } else {
        unix_uri(&text)
    };
    uri.parse().expect("a generated file URI is always valid")
}

fn file_path(host: &str, path: &str) -> PathBuf {
    if !cfg!(windows) {
        return PathBuf::from(path);
    }
    if host.is_empty() || host.eq_ignore_ascii_case("localhost") {
        return PathBuf::from(windows_drive_path(path));
    }
    PathBuf::from(format!("\\\\{host}{}", path.replace('/', "\\")))
}

/// `/c:/repo/a.ts` becomes `c:/repo/a.ts`; anything else is left alone.
fn windows_drive_path(path: &str) -> &str {
    let bytes = path.as_bytes();
    if bytes.len() >= 3 && bytes[0] == b'/' && bytes[1].is_ascii_alphabetic() && bytes[2] == b':'
    {
        return &path[1..];
    }
    path
}

fn unix_uri(path: &str) -> String {
    if path.starts_with('/') {
        format!("file://{}", encode_path(path))
    } else {
        format!("file:///{}", encode_path(path))
    }
}

/// `C:\repo\a.ts` becomes `file:///C:/repo/a.ts`; a `\\server\share` path becomes
/// `file://server/share`, both with forward slashes.
fn windows_uri(path: &str) -> String {
    let path = path.replace('\\', "/");
    let Some(rest) = path.strip_prefix("//") else {
        return format!("file:///{}", encode_path(&path));
    };
    let (host, tail) = match rest.find('/') {
        Some(index) => (&rest[..index], &rest[index..]),
        None => (rest, ""),
    };
    format!("file://{}{}", encode_path(host), encode_path(tail))
}

/// Percent-encodes a path while keeping the separator and the Windows drive colon.
fn encode_path(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        if is_safe_path_byte(byte) {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn is_safe_path_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/' | b':' | b'@')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uri(text: &str) -> Uri {
        text.parse().expect("valid uri")
    }

    #[test]
    fn converts_plain_file_uris() {
        assert_eq!(
            to_file_path(&uri("file:///tmp/project/a.ts")),
            Some(PathBuf::from("/tmp/project/a.ts"))
        );
    }

    #[test]
    fn decodes_percent_encoded_paths() {
        assert_eq!(
            to_file_path(&uri("file:///tmp/my%20project/a.ts")),
            Some(PathBuf::from("/tmp/my project/a.ts"))
        );
    }

    #[test]
    fn rejects_non_file_schemes() {
        assert_eq!(to_file_path(&uri("https://example.com/a.ts")), None);
        assert_eq!(to_file_path(&uri("untitled:Untitled-1")), None);
    }

    #[test]
    fn windows_drive_detection_strips_the_leading_slash() {
        assert_eq!(windows_drive_path("/c:/repo/a.ts"), "c:/repo/a.ts");
        assert_eq!(windows_drive_path("/repo/a.ts"), "/repo/a.ts");
    }

    #[cfg(not(windows))]
    #[test]
    fn from_file_path_round_trips_a_plain_path() {
        let path = PathBuf::from("/tmp/project/a.ts");
        let generated = from_file_path(&path);

        assert_eq!(generated.as_str(), "file:///tmp/project/a.ts");
        assert_eq!(to_file_path(&generated), Some(path));
    }

    #[test]
    fn from_file_path_round_trips_spaces_and_accents() {
        let dir = std::env::temp_dir().join("nullscan uri ñ test");
        std::fs::create_dir_all(&dir).expect("create dir");
        let path = dir.join("a b ñ.ts");

        let generated = from_file_path(&path);

        assert_eq!(to_file_path(&generated), Some(path));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(not(windows))]
    #[test]
    fn from_file_path_encodes_spaces_and_reserved_characters() {
        let path = PathBuf::from("/tmp/my project/a#b?.ts");
        let generated = from_file_path(&path);

        assert_eq!(
            generated.as_str(),
            "file:///tmp/my%20project/a%23b%3F.ts"
        );
        assert_eq!(to_file_path(&generated), Some(path));
    }

    #[cfg(not(windows))]
    #[test]
    fn from_file_path_encodes_non_ascii_characters() {
        let path = PathBuf::from("/tmp/año/a.ts");
        let generated = from_file_path(&path);

        assert_eq!(generated.as_str(), "file:///tmp/a%C3%B1o/a.ts");
        assert_eq!(to_file_path(&generated), Some(path));
    }

    #[test]
    fn windows_paths_become_forward_slash_uris() {
        assert_eq!(windows_uri(r"C:\repo\a.ts"), "file:///C:/repo/a.ts");
        assert_eq!(
            windows_uri(r"\\server\share\a.ts"),
            "file://server/share/a.ts"
        );
        assert_eq!(windows_uri(r"C:\my project\a.ts"), "file:///C:/my%20project/a.ts");
    }

    #[cfg(windows)]
    #[test]
    fn windows_from_file_path_uses_a_forward_slash_drive_uri() {
        let path = PathBuf::from(r"C:\repo\a.ts");
        let generated = from_file_path(&path);

        assert_eq!(generated.as_str(), "file:///C:/repo/a.ts");
        assert_eq!(to_file_path(&generated), Some(path));
    }
}
