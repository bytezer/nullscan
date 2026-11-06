//! Minimal `file:` URI to path conversion for LSP document URIs.
//!
//! `lsp-types` 0.97 wraps `fluent-uri`, which has no `to_file_path`. Only the direction
//! the server needs is implemented: incoming document URIs to real paths, with percent
//! decoding and drive/UNC handling on Windows.

use std::path::PathBuf;

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
}
