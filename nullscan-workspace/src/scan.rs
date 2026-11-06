//! One full scan: walk, read, analyze, detect duplicates, and cap the result.
//!
//! Both binaries call this module so the CLI and the editor integration see the same
//! files, the same config, the same rule order and the same truncation behavior.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use nullscan_core::{
    Config, Diagnostic, Duplicate, Language, analyze, find_duplicates,
};
use rayon::prelude::*;

use crate::walk;

/// Everything a caller can control about a scan.
pub struct ScanOptions<'a> {
    pub paths: &'a [PathBuf],
    pub config: Config,
    pub config_path: Option<PathBuf>,
    pub max_diagnostics: Option<usize>,
}

/// One successfully read and analyzed file, in input order.
pub struct FileResult {
    pub file: PathBuf,
    pub language: Language,
    pub source: String,
    pub diagnostics: Vec<Diagnostic>,
}

/// The complete result of a scan, ready for text, SARIF or JSON rendering.
pub struct ScanReport {
    pub files: Vec<FileResult>,
    pub duplicates: Vec<Duplicate>,
    pub warnings: Vec<String>,
    pub duration: Duration,
    pub truncated: bool,
    pub config_path: Option<PathBuf>,
    pub disabled_rules: Vec<String>,
}

/// Scans every supported file under `options.paths`.
///
/// Reading and analysis run in parallel, but `rayon`'s ordered `collect` keeps the
/// report in input order. Read failures become warnings instead of aborting the run.
pub fn scan(options: ScanOptions<'_>) -> ScanReport {
    let started = Instant::now();
    let walked = walk::collect_files(options.paths);
    let mut warnings = walked.warnings;

    let results: Vec<Result<FileResult, String>> = walked
        .files
        .par_iter()
        .map(|(file, language)| read_and_analyze(file, *language, &options.config))
        .collect();

    let mut files = Vec::with_capacity(results.len());
    for result in results {
        match result {
            Ok(file) => files.push(file),
            Err(warning) => warnings.push(warning),
        }
    }

    let duplicates = find_duplicates_if_enabled(&files, &options.config);
    let total_diagnostics: usize = files.iter().map(|file| file.diagnostics.len()).sum();
    let truncated = cap_diagnostics(&mut files, options.max_diagnostics, total_diagnostics);

    warnings.sort();
    warnings.dedup();

    ScanReport {
        files,
        duplicates,
        warnings,
        duration: started.elapsed(),
        truncated,
        config_path: options.config_path,
        disabled_rules: sorted_disabled_rules(&options.config),
    }
}

/// Serializes a report as the machine-readable JSON contract.
pub fn render_json(report: &ScanReport) -> String {
    crate::json::render(report)
}

fn read_and_analyze(file: &Path, language: Language, config: &Config) -> Result<FileResult, String> {
    let source = fs::read_to_string(file)
        .map_err(|error| format!("error: could not read {}: {error}", file.display()))?;
    let diagnostics = analyze(&source, language, config);
    Ok(FileResult {
        file: file.to_path_buf(),
        language,
        source,
        diagnostics,
    })
}

fn find_duplicates_if_enabled(files: &[FileResult], config: &Config) -> Vec<Duplicate> {
    if !config.rule_enabled("duplicate-code") {
        return Vec::new();
    }
    let sources: Vec<(PathBuf, Language, String)> = files
        .iter()
        .map(|file| (file.file.clone(), file.language, file.source.clone()))
        .collect();
    find_duplicates(&sources, config.duplicate_code_min_tokens)
}

/// Keeps the first `max` diagnostics in file order. Returns whether any were dropped.
fn cap_diagnostics(
    files: &mut [FileResult],
    max_diagnostics: Option<usize>,
    total: usize,
) -> bool {
    let Some(max) = max_diagnostics else {
        return false;
    };
    if total <= max {
        return false;
    }

    let mut remaining = max;
    for file in files {
        let keep = remaining.min(file.diagnostics.len());
        file.diagnostics.truncate(keep);
        remaining -= keep;
    }
    true
}

fn sorted_disabled_rules(config: &Config) -> Vec<String> {
    let mut disabled: Vec<String> = config.disabled_rules.iter().cloned().collect();
    disabled.sort();
    disabled
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
            "nullscan-scan-{label}-{}-{id}",
            process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn options<'a>(paths: &'a [PathBuf], config: Config, max: Option<usize>) -> ScanOptions<'a> {
        ScanOptions {
            paths,
            config,
            config_path: None,
            max_diagnostics: max,
        }
    }

    #[test]
    fn scans_files_and_applies_the_diagnostic_cap() {
        let root = temp_dir("cap");
        fs::write(
            root.join("a.ts"),
            "console.log(1);\nconsole.log(2);\n",
        )
        .expect("write a.ts");
        fs::write(root.join("b.ts"), "console.log(3);\n").expect("write b.ts");

        let config = Config::default();
        let mut capped = config;
        capped.disabled_rules.insert("duplicate-code".to_string());
        let report = scan(options(&[root.clone()], capped, Some(2)));

        assert_eq!(report.files.len(), 2);
        assert_eq!(report.files[0].diagnostics.len(), 2);
        assert!(report.files[1].diagnostics.is_empty());
        assert!(report.truncated);
        assert!(report.duplicates.is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn read_failures_become_warnings() {
        let root = temp_dir("read");
        let file = root.join("invalid-utf8.ts");
        fs::write(&file, [0xff, 0xfe]).expect("write invalid source");

        let report = scan(options(&[file.clone()], Config::default(), None));

        assert!(report.files.is_empty());
        assert_eq!(report.warnings.len(), 1);
        assert!(report.warnings[0].starts_with("error: could not read "));
        assert!(!report.truncated);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn disabled_rules_are_sorted() {
        let root = temp_dir("disabled");
        let mut config = Config::default();
        config.disabled_rules.insert("no-var".to_string());
        config.disabled_rules.insert("eqeqeq".to_string());

        let report = scan(options(&[root.clone()], config, None));

        assert_eq!(report.disabled_rules, vec!["eqeqeq", "no-var"]);
        let _ = fs::remove_dir_all(root);
    }
}
