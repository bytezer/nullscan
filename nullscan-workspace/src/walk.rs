//! Filesystem collection shared by every nullscan consumer.
//!
//! Inputs are resolved to supported TypeScript files once, in one place, so a CLI run
//! and an editor workspace scan cannot disagree about which files exist. Results are
//! sorted and deduplicated: the walk itself depends on filesystem ordering, but every
//! consumer downstream expects deterministic output.

use std::fs;
use std::path::{Path, PathBuf};

use nullscan_core::Language;

use crate::ignore::PathFilter;

/// Directories that never contain hand-written source: skipping them keeps duplication
/// detection from comparing a file against its own compiled/generated output.
pub const SKIPPED_DIRS: &[&str] = &["node_modules", "dist", "build", ".next", "out"];

/// Files found by a walk plus the user-facing problems encountered on the way.
pub struct WalkResult {
    pub files: Vec<(PathBuf, Language)>,
    pub warnings: Vec<String>,
}

/// Resolves every input path to supported source files.
///
/// Explicit files get a warning when their extension is unsupported or the ignore filter
/// excludes them; directories are walked recursively, pruning ignored directories and
/// silently skipping ignored files, unsupported entries, `SKIPPED_DIRS` and dot-directories.
/// Missing paths and unreadable directories become warnings so a scan with one bad input
/// still processes the rest.
pub fn collect_files(paths: &[PathBuf], ignore: Option<&PathFilter>) -> WalkResult {
    let mut files = Vec::new();
    let mut warnings = Vec::new();

    for path in paths {
        if is_ignored(ignore, path) {
            warnings.push(format!(
                "ignoring {}: excluded by nullscan.toml",
                path.display()
            ));
            continue;
        }
        collect_path(path, ignore, &mut files, &mut warnings);
    }

    files.sort();
    files.dedup();
    warnings.sort();
    warnings.dedup();
    WalkResult { files, warnings }
}

/// Whether at least one input path exists, used by callers that treat "nothing to scan"
/// as a tool error rather than a clean run.
pub fn any_existing_path(paths: &[PathBuf]) -> bool {
    paths.iter().any(|path| path.exists())
}

fn collect_path(
    path: &Path,
    ignore: Option<&PathFilter>,
    files: &mut Vec<(PathBuf, Language)>,
    warnings: &mut Vec<String>,
) {
    if path.is_file() {
        collect_file(path, files, warnings);
        return;
    }
    if !path.is_dir() {
        warnings.push(format!("error: {} does not exist", path.display()));
        return;
    }

    let Ok(entries) = fs::read_dir(path) else {
        warnings.push(format!("error: could not read directory {}", path.display()));
        return;
    };
    for entry in entries.flatten() {
        collect_entry(&entry.path(), ignore, files, warnings);
    }
}

fn collect_file(path: &Path, files: &mut Vec<(PathBuf, Language)>, warnings: &mut Vec<String>) {
    match Language::from_path(path) {
        Some(language) => files.push((path.to_path_buf(), language)),
        None => warnings.push(format!(
            "warning: skipping {}: unsupported file type",
            path.display()
        )),
    }
}

fn collect_entry(
    path: &Path,
    ignore: Option<&PathFilter>,
    files: &mut Vec<(PathBuf, Language)>,
    warnings: &mut Vec<String>,
) {
    if is_ignored(ignore, path) {
        return;
    }
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    if name.starts_with('.') || SKIPPED_DIRS.contains(&name.as_str()) {
        return;
    }
    if path.is_dir() {
        collect_path(path, ignore, files, warnings);
    } else if let Some(language) = Language::from_path(path) {
        files.push((path.to_path_buf(), language));
    }
}

fn is_ignored(ignore: Option<&PathFilter>, path: &Path) -> bool {
    ignore.is_some_and(|filter| filter.matches(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::process;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn temp_dir(label: &str) -> PathBuf {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "nullscan-walk-{label}-{}-{id}",
            process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn touch(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent dir");
        }
        fs::write(path, "").expect("write fixture file");
    }

    fn names(files: &[(PathBuf, Language)]) -> Vec<String> {
        files
            .iter()
            .map(|(path, _)| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn skips_dot_dirs_and_known_generated_dirs() {
        let root = temp_dir("skip");
        touch(&root.join("src/a.ts"));
        touch(&root.join("src/b.tsx"));
        touch(&root.join("node_modules/dep/index.ts"));
        touch(&root.join("dist/out.ts"));
        touch(&root.join(".hidden/secret.ts"));

        let result = collect_files(&[root.clone()], None);

        assert_eq!(names(&result.files), vec!["a.ts", "b.tsx"]);
        assert!(result.warnings.is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn explicit_unsupported_file_is_a_warning() {
        let root = temp_dir("unsupported");
        let script = root.join("script.js");
        touch(&script);

        let result = collect_files(&[script.clone()], None);

        assert!(result.files.is_empty());
        assert_eq!(
            result.warnings,
            vec![format!(
                "warning: skipping {}: unsupported file type",
                script.display()
            )]
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn result_order_is_deterministic_and_deduplicated() {
        let root = temp_dir("order");
        touch(&root.join("b.ts"));
        touch(&root.join("a.ts"));
        let file = root.join("a.ts");

        let result = collect_files(&[root.clone(), file.clone(), root.clone()], None);

        assert_eq!(names(&result.files), vec!["a.ts", "b.ts"]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn missing_path_is_reported() {
        let root = temp_dir("missing");
        let missing = root.join("nope");

        let result = collect_files(&[missing.clone()], None);

        assert!(result.files.is_empty());
        assert_eq!(
            result.warnings,
            vec![format!("error: {} does not exist", missing.display())]
        );
        assert!(!any_existing_path(&[missing]));
        assert!(any_existing_path(&[root.clone()]));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ignore_filter_prunes_directories_and_warns_for_explicit_inputs() {
        let root = temp_dir("ignore");
        touch(&root.join("src/a.ts"));
        touch(&root.join("tests/b.ts"));
        let (filter, warnings) = PathFilter::new(root.clone(), &["tests".to_string()]);
        assert!(warnings.is_empty());

        let walked = collect_files(&[root.clone()], Some(&filter));
        assert_eq!(names(&walked.files), vec!["a.ts"]);
        assert!(walked.warnings.is_empty());

        let explicit = root.join("tests/b.ts");
        let direct = collect_files(&[explicit.clone()], Some(&filter));
        assert!(direct.files.is_empty());
        assert_eq!(
            direct.warnings,
            vec![format!(
                "ignoring {}: excluded by nullscan.toml",
                explicit.display()
            )]
        );
        let _ = fs::remove_dir_all(root);
    }
}
