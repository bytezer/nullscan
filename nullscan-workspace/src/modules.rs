//! Resolves relative imports to module summaries for cross-file rules.
//!
//! Only `./` and `../` specifiers are followed; bare specifiers (packages, tsconfig
//! `paths`, `node_modules` declarations) return `None`. Open-buffer overrides win over
//! disk, and each module is parsed once per resolver, so create one per scan.

use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

use nullscan_core::{Language, ModuleResolver, ModuleSummary, summarize};

const SUFFIXES: &[&str] = &[
    "", ".ts", ".tsx", ".mts", ".cts", ".d.ts", "/index.ts", "/index.tsx",
];

pub struct WorkspaceResolver<'a> {
    overrides: Option<&'a HashMap<PathBuf, String>>,
    // ponytail: a single mutex around the cache; shard it if parallel scans contend on it.
    cache: Mutex<HashMap<PathBuf, Option<Arc<ModuleSummary>>>>,
}

impl<'a> WorkspaceResolver<'a> {
    pub fn new(overrides: Option<&'a HashMap<PathBuf, String>>) -> Self {
        Self { overrides, cache: Mutex::new(HashMap::new()) }
    }

    fn is_file(&self, path: &Path) -> bool {
        self.overrides.is_some_and(|map| map.contains_key(path)) || path.is_file()
    }

    fn read(&self, path: &Path) -> Option<String> {
        match self.overrides.and_then(|map| map.get(path)) {
            Some(text) => Some(text.clone()),
            None => fs::read_to_string(path).ok(),
        }
    }

    fn load(&self, path: &Path) -> Option<Arc<ModuleSummary>> {
        if let Some(hit) = self.cache.lock().ok()?.get(path) {
            return hit.clone();
        }
        let language = Language::from_path(path)?;
        let summary = self
            .read(path)
            .and_then(|source| summarize(&source, language, path.to_path_buf()))
            .map(Arc::new);
        // Two threads may parse the same module once; the results are identical.
        self.cache.lock().ok()?.insert(path.to_path_buf(), summary.clone());
        summary
    }
}

impl ModuleResolver for WorkspaceResolver<'_> {
    fn summary(&self, from: &Path, specifier: &str) -> Option<Arc<ModuleSummary>> {
        if !(specifier.starts_with("./") || specifier.starts_with("../")) {
            return None;
        }
        let base = normalize(&from.parent()?.join(specifier));
        let found = SUFFIXES.iter().find_map(|suffix| {
            let mut candidate: OsString = base.clone().into_os_string();
            candidate.push(suffix);
            let candidate = PathBuf::from(candidate);
            self.is_file(&candidate).then_some(candidate)
        })?;
        self.load(&found)
    }
}

/// Resolves `.` and `..` lexically so the result matches the walker's paths.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir if out.file_name().is_some() => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_collapses_dot_segments() {
        assert_eq!(normalize(Path::new("/a/b/../c/./d")), PathBuf::from("/a/c/d"));
        assert_eq!(normalize(Path::new("a/../../x")), PathBuf::from("../x"));
    }

    #[test]
    fn bare_specifiers_are_not_resolved() {
        let resolver = WorkspaceResolver::new(None);
        assert!(resolver.summary(Path::new("/a/b.ts"), "react").is_none());
    }

    #[test]
    fn overrides_win_and_index_files_resolve() {
        let overrides = HashMap::from([(
            PathBuf::from("/p/types/index.ts"),
            "export type T = { m: () => Promise<void> };".to_string(),
        )]);
        let resolver = WorkspaceResolver::new(Some(&overrides));
        let found = resolver.summary(Path::new("/p/src/a.ts"), "../types").expect("index.ts");
        assert_eq!(found.path.as_deref(), Some(Path::new("/p/types/index.ts")));
    }
}
