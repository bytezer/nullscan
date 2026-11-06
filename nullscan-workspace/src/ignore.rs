//! Path exclusion filters for `nullscan.toml`.
//!
//! Patterns are relative to a root directory (the config file's directory, the workspace
//! folder, or the process working directory). A pattern without a slash matches any path
//! segment, while a pattern with a slash matches the whole path relative to the root.
//! `*` and `?` stay inside one segment, `**` spans segments, and a trailing slash means
//! the whole subtree. There is no negation and no character-class support.

use std::path::{Path, PathBuf};

/// A compiled set of ignore patterns plus the root they are relative to.
pub struct PathFilter {
    root: PathBuf,
    root_text: String,
    patterns: Vec<IgnorePattern>,
}

#[derive(Debug, Clone)]
struct IgnorePattern {
    text: String,
    kind: PatternKind,
    /// The pattern had a trailing slash, so it also matches everything below `text`.
    subtree: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PatternKind {
    /// No slash: match any single path segment.
    Segment,
    /// Slash present: match the path relative to the root.
    Relative,
    /// Leading slash or a Windows drive: match the absolute path.
    Absolute,
}

impl PathFilter {
    /// Compiles `patterns` against `root`.
    ///
    /// Returns the filter and one warning per unsupported pattern (currently `!`-prefixed
    /// negation, which is ignored).
    pub fn new(root: impl Into<PathBuf>, patterns: &[String]) -> (Self, Vec<String>) {
        let root = absolute(root.into());
        let root_text = normalize_path_text(&root.to_string_lossy());
        let mut compiled = Vec::with_capacity(patterns.len());
        let mut warnings = Vec::new();

        for raw in patterns {
            match compile(raw) {
                Ok(Some(pattern)) => compiled.push(pattern),
                Ok(None) => {}
                Err(warning) => warnings.push(warning),
            }
        }

        (
            Self {
                root,
                root_text,
                patterns: compiled,
            },
            warnings,
        )
    }

    /// Whether this filter has no usable patterns.
    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }

    /// Whether `path` is excluded. Paths outside the root can only match an absolute
    /// pattern.
    pub fn matches(&self, path: &Path) -> bool {
        if self.patterns.is_empty() {
            return false;
        }
        let absolute = self.absolute_text(path);
        let relative = relative_text(&self.root_text, &absolute);
        self.patterns
            .iter()
            .any(|pattern| pattern.matches(&absolute, relative.as_deref()))
    }

    fn absolute_text(&self, path: &Path) -> String {
        let raw = normalize_slashes(&path.to_string_lossy());
        if is_absolute_text(&raw) {
            return normalize_path_text(&raw);
        }
        // A relative input comes from the process working directory, but tests and callers
        // may also hand in paths that are already expressed relative to the filter root.
        if let Ok(cwd) = std::path::absolute(path) {
            let text = normalize_path_text(&cwd.to_string_lossy());
            if relative_text(&self.root_text, &text).is_some() {
                return text;
            }
        }
        let joined = self.root.join(path);
        normalize_path_text(&joined.to_string_lossy())
    }
}

impl IgnorePattern {
    fn matches(&self, absolute: &str, relative: Option<&str>) -> bool {
        match self.kind {
            PatternKind::Segment => relative.is_some_and(|relative| {
                relative
                    .split('/')
                    .filter(|segment| !segment.is_empty())
                    .any(|segment| glob_segment(&self.text, segment))
            }),
            PatternKind::Relative => {
                relative.is_some_and(|relative| full_path_matches(&self.text, self.subtree, relative))
            }
            PatternKind::Absolute => full_path_matches(&self.text, self.subtree, absolute),
        }
    }
}

fn compile(raw: &str) -> Result<Option<IgnorePattern>, String> {
    let normalized = normalize_slashes(raw.trim());
    if normalized.is_empty() {
        return Ok(None);
    }
    if normalized.starts_with('!') {
        return Err(format!(
            "warning: unsupported ignore pattern '{raw}': negation is not supported"
        ));
    }

    let subtree = normalized.ends_with('/');
    let text = if normalized == "/" {
        normalized.as_str()
    } else {
        normalized.trim_end_matches('/')
    };
    if text.is_empty() {
        return Ok(None);
    }

    let kind = if is_absolute_text(text) {
        PatternKind::Absolute
    } else if text.contains('/') {
        PatternKind::Relative
    } else {
        PatternKind::Segment
    };

    Ok(Some(IgnorePattern {
        text: text.to_string(),
        kind,
        subtree,
    }))
}

fn full_path_matches(pattern: &str, subtree: bool, path: &str) -> bool {
    if glob_segments(pattern, path) {
        return true;
    }
    subtree && glob_segments(&format!("{pattern}/**"), path)
}

fn glob_segments(pattern: &str, path: &str) -> bool {
    let pattern: Vec<&str> = pattern.split('/').collect();
    let path: Vec<&str> = if path.is_empty() {
        Vec::new()
    } else {
        path.split('/').collect()
    };

    // `table[i][j]` is true when `pattern[i..]` matches `path[j..]`.
    let mut table = vec![vec![false; path.len() + 1]; pattern.len() + 1];
    table[pattern.len()][path.len()] = true;

    for i in (0..pattern.len()).rev() {
        for j in (0..=path.len()).rev() {
            table[i][j] = if pattern[i] == "**" {
                table[i + 1][j] || (j < path.len() && table[i][j + 1])
            } else {
                j < path.len() && glob_segment(pattern[i], path[j]) && table[i + 1][j + 1]
            };
        }
    }

    table[0][0]
}

/// Matches one path segment; `*` matches any run inside the segment and `?` one character.
fn glob_segment(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let (mut p, mut t) = (0, 0);
    let mut star: Option<usize> = None;
    let mut resume = 0;

    while t < text.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some(p);
            resume = t + 1;
            p += 1;
        } else if let Some(star_index) = star {
            p = star_index + 1;
            t = resume;
            resume += 1;
        } else {
            return false;
        }
    }

    while p < pattern.len() && pattern[p] == '*' {
        p += 1;
    }
    p == pattern.len()
}

fn relative_text(root: &str, absolute: &str) -> Option<String> {
    if absolute == root {
        return Some(String::new());
    }
    if root == "/" {
        return absolute.strip_prefix('/').map(str::to_string);
    }
    absolute
        .strip_prefix(root)
        .and_then(|rest| rest.strip_prefix('/'))
        .map(str::to_string)
}

fn absolute(path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        return path;
    }
    std::path::absolute(&path).unwrap_or(path)
}

fn normalize_slashes(value: &str) -> String {
    value.replace('\\', "/")
}

/// Lexically removes `.` and `..` components and drops trailing slashes.
fn normalize_path_text(value: &str) -> String {
    let value = normalize_slashes(value);
    let absolute = value.starts_with('/');
    let mut parts: Vec<&str> = Vec::new();

    for segment in value.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if parts.last().is_some_and(|last| *last != "..") {
                    parts.pop();
                } else if !absolute {
                    parts.push("..");
                }
            }
            other => parts.push(other),
        }
    }

    let joined = parts.join("/");
    if absolute {
        format!("/{joined}")
    } else {
        joined
    }
}

fn is_absolute_text(value: &str) -> bool {
    value.starts_with('/') || has_windows_drive(value)
}

fn has_windows_drive(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_filter(patterns: &[&str]) -> PathFilter {
        let owned: Vec<String> = patterns.iter().map(|pattern| (*pattern).to_string()).collect();
        let (filter, warnings) = PathFilter::new("/repo", &owned);
        assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
        filter
    }

    #[test]
    fn empty_filter_matches_nothing() {
        let filter = make_filter(&[]);

        assert!(filter.is_empty());
        assert!(!filter.matches(Path::new("/repo/src/a.ts")));
    }

    #[test]
    fn segment_patterns_match_any_segment() {
        let filter = make_filter(&["tests"]);

        assert!(filter.matches(Path::new("/repo/tests/a.ts")));
        assert!(filter.matches(Path::new("/repo/src/tests/a.ts")));
        assert!(filter.matches(Path::new("/repo/src/tests")));
        assert!(!filter.matches(Path::new("/repo/src/testing/a.ts")));
    }

    #[test]
    fn file_glob_without_slash_matches_anywhere() {
        let filter = make_filter(&["*.test.ts"]);

        assert!(filter.matches(Path::new("/repo/a.test.ts")));
        assert!(filter.matches(Path::new("/repo/src/deep/a.test.ts")));
        assert!(!filter.matches(Path::new("/repo/src/a.ts")));
    }

    #[test]
    fn relative_patterns_match_the_whole_path() {
        let filter = make_filter(&["src/generated/**"]);

        assert!(filter.matches(Path::new("/repo/src/generated/a.ts")));
        assert!(filter.matches(Path::new("/repo/src/generated/deep/a.ts")));
        assert!(!filter.matches(Path::new("/repo/generated/a.ts")));
        assert!(!filter.matches(Path::new("/repo/src/generated-file.ts")));
    }

    #[test]
    fn double_star_matches_zero_or_more_segments() {
        let filter = make_filter(&["**/*.test.ts"]);

        assert!(filter.matches(Path::new("/repo/a.test.ts")));
        assert!(filter.matches(Path::new("/repo/src/a.test.ts")));
        assert!(filter.matches(Path::new("/repo/src/deep/a.test.ts")));

        let middle = make_filter(&["src/**/generated.ts"]);
        assert!(middle.matches(Path::new("/repo/src/generated.ts")));
        assert!(middle.matches(Path::new("/repo/src/deep/nested/generated.ts")));
        assert!(!middle.matches(Path::new("/repo/other/generated.ts")));
    }

    #[test]
    fn question_mark_matches_one_character() {
        let filter = make_filter(&["src/?.ts"]);

        assert!(filter.matches(Path::new("/repo/src/a.ts")));
        assert!(!filter.matches(Path::new("/repo/src/ab.ts")));
        assert!(!filter.matches(Path::new("/repo/src/.ts")));
    }

    #[test]
    fn trailing_slash_matches_the_whole_subtree() {
        let segment = make_filter(&["tests/"]);
        assert!(segment.matches(Path::new("/repo/tests/a.ts")));
        assert!(segment.matches(Path::new("/repo/src/tests/a.ts")));

        let relative = make_filter(&["src/generated/"]);
        assert!(relative.matches(Path::new("/repo/src/generated")));
        assert!(relative.matches(Path::new("/repo/src/generated/a.ts")));
        assert!(relative.matches(Path::new("/repo/src/generated/deep/a.ts")));
        assert!(!relative.matches(Path::new("/repo/src/generated-file.ts")));
    }

    #[test]
    fn backslashes_are_normalized_in_patterns_and_paths() {
        let filter = make_filter(&["src\\generated\\**"]);

        assert!(filter.matches(Path::new("/repo/src/generated/a.ts")));
        assert!(filter.matches(Path::new("src\\generated\\a.ts")));
    }

    #[test]
    fn absolute_patterns_match_absolute_paths() {
        let filter = make_filter(&["/repo/tests/**"]);

        assert!(filter.matches(Path::new("/repo/tests/a.ts")));
        assert!(!filter.matches(Path::new("/repo/src/tests/a.ts")));

        let windows = make_filter(&["C:/repo/tests/**"]);
        assert!(windows.matches(Path::new("C:/repo/tests/a.ts")));
        assert!(!windows.matches(Path::new("C:/repo/src/tests/a.ts")));
    }

    #[test]
    fn paths_outside_the_root_only_match_absolute_patterns() {
        let relative = make_filter(&["tests"]);
        assert!(!relative.matches(Path::new("/elsewhere/tests/a.ts")));

        let absolute = make_filter(&["/elsewhere/tests/**"]);
        assert!(absolute.matches(Path::new("/elsewhere/tests/a.ts")));
    }

    #[test]
    fn negation_is_ignored_with_a_warning() {
        let patterns = ["!tests".to_string(), "generated".to_string()];
        let (filter, warnings) = PathFilter::new("/repo", &patterns);

        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("'!tests'"));
        assert!(warnings[0].contains("negation is not supported"));
        assert!(!filter.is_empty());
        assert!(filter.matches(Path::new("/repo/src/generated/a.ts")));
        assert!(!filter.matches(Path::new("/repo/tests/a.ts")));
    }
}
