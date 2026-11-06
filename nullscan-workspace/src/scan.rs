//! One full scan: walk, read, analyze, detect duplicates, and cap the result.
//!
//! Both binaries call this module so the CLI and the editor integration see the same
//! files, the same config, the same rule order and the same truncation behavior.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use nullscan_core::{
    Config, Diagnostic, Duplicate, DuplicateOptions, Language, analyze, find_duplicates,
};
use rayon::prelude::*;

use crate::ignore::PathFilter;
use crate::walk;

/// Everything a caller can control about a scan.
pub struct ScanOptions<'a> {
    pub paths: &'a [PathBuf],
    pub config: Config,
    pub config_path: Option<PathBuf>,
    pub max_diagnostics: Option<usize>,
    /// In-memory text per file that wins over the bytes on disk, used for dirty editor
    /// buffers. Paths must match the ones the walker produces.
    pub overrides: Option<&'a HashMap<PathBuf, String>>,
    /// Checked before analyzing each file; once set the scan returns early.
    pub cancel: Option<&'a AtomicBool>,
    /// Files excluded from both the walk and per-file analysis.
    pub files_ignore: Option<&'a PathFilter>,
    /// Files excluded only from clone detection; they are still analyzed by the rules.
    pub duplicates_ignore: Option<&'a PathFilter>,
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
    /// Set when a cancellation was observed, so the caller can discard partial results.
    /// Not part of the JSON contract.
    pub cancelled: bool,
    pub config_path: Option<PathBuf>,
    pub disabled_rules: Vec<String>,
}

/// Scans every supported file under `options.paths`.
///
/// Reading and analysis run in parallel, but `rayon`'s ordered `collect` keeps the
/// report in input order. Read failures become warnings instead of aborting the run.
/// A set `cancel` flag stops the scan before the next file; results collected so far are
/// kept and [`ScanReport::cancelled`] says they are partial.
pub fn scan(options: ScanOptions<'_>) -> ScanReport {
    let started = Instant::now();
    let walked = walk::collect_files(options.paths, options.files_ignore);
    let mut warnings = walked.warnings;

    let results: Vec<Option<Result<FileResult, String>>> = walked
        .files
        .par_iter()
        .map(|(file, language)| {
            if is_cancelled(options.cancel) {
                return None;
            }
            Some(read_and_analyze(
                file,
                *language,
                &options.config,
                options.overrides,
            ))
        })
        .collect();

    let cancelled = results.iter().any(Option::is_none);
    let mut files = Vec::with_capacity(results.len());
    for result in results.into_iter().flatten() {
        match result {
            Ok(file) => files.push(file),
            Err(warning) => warnings.push(warning),
        }
    }

    let duplicates = if cancelled {
        Vec::new()
    } else {
        find_duplicates_if_enabled(&files, &options.config, options.duplicates_ignore)
    };
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
        cancelled,
        config_path: options.config_path,
        disabled_rules: sorted_disabled_rules(&options.config),
    }
}

/// Combines the per-folder reports of a multi-root workspace.
///
/// Every folder resolves its own `nullscan.toml`, so a merged report has no single
/// config: `config_path` stays `None` unless exactly one folder contributed a file.
/// Files are concatenated in folder order, duplicate spans and warnings are deduplicated,
/// and `truncated`/`cancelled` are ORed so a partial folder taints the whole run.
pub fn merge(reports: Vec<ScanReport>) -> ScanReport {
    let mut files = Vec::new();
    let mut duplicates = Vec::new();
    let mut seen_duplicates = HashSet::new();
    let mut warnings = Vec::new();
    let mut disabled_rules = BTreeSet::new();
    let mut duration = Duration::ZERO;
    let mut truncated = false;
    let mut cancelled = false;
    let mut config_paths = Vec::new();

    for report in reports {
        files.extend(report.files);
        for duplicate in report.duplicates {
            if seen_duplicates.insert(duplicate_key(&duplicate)) {
                duplicates.push(duplicate);
            }
        }
        warnings.extend(report.warnings);
        disabled_rules.extend(report.disabled_rules);
        duration += report.duration;
        truncated |= report.truncated;
        cancelled |= report.cancelled;
        if let Some(path) = report.config_path {
            config_paths.push(path);
        }
    }

    warnings.sort();
    warnings.dedup();

    ScanReport {
        files,
        duplicates,
        warnings,
        duration,
        truncated,
        cancelled,
        config_path: single_config_path(config_paths),
        disabled_rules: disabled_rules.into_iter().collect(),
    }
}

/// Serializes a report as the machine-readable JSON contract.
pub fn render_json(report: &ScanReport) -> String {
    crate::json::render(report)
}

fn is_cancelled(cancel: Option<&AtomicBool>) -> bool {
    cancel.is_some_and(|flag| flag.load(Ordering::Relaxed))
}

fn read_and_analyze(
    file: &Path,
    language: Language,
    config: &Config,
    overrides: Option<&HashMap<PathBuf, String>>,
) -> Result<FileResult, String> {
    let source = read_source(file, overrides)?;
    let diagnostics = analyze(&source, language, config);
    Ok(FileResult {
        file: file.to_path_buf(),
        language,
        source,
        diagnostics,
    })
}

/// Prefers an in-memory buffer so an unsaved editor document is analyzed as shown.
fn read_source(
    file: &Path,
    overrides: Option<&HashMap<PathBuf, String>>,
) -> Result<String, String> {
    if let Some(text) = overrides.and_then(|map| map.get(file)) {
        return Ok(text.clone());
    }
    fs::read_to_string(file)
        .map_err(|error| format!("error: could not read {}: {error}", file.display()))
}

/// Order-independent identity of a duplicate, so the same span pair reported by two
/// folders is kept once.
type DuplicateKey = (PathBuf, usize, usize, PathBuf, usize, usize);

fn duplicate_key(duplicate: &Duplicate) -> DuplicateKey {
    let first = (
        duplicate.a.file.clone(),
        duplicate.a.start_line,
        duplicate.a.end_line,
    );
    let second = (
        duplicate.b.file.clone(),
        duplicate.b.start_line,
        duplicate.b.end_line,
    );
    if first <= second {
        (first.0, first.1, first.2, second.0, second.1, second.2)
    } else {
        (second.0, second.1, second.2, first.0, first.1, first.2)
    }
}

fn single_config_path(paths: Vec<PathBuf>) -> Option<PathBuf> {
    if paths.len() == 1 {
        paths.into_iter().next()
    } else {
        None
    }
}

fn find_duplicates_if_enabled(
    files: &[FileResult],
    config: &Config,
    ignore: Option<&PathFilter>,
) -> Vec<Duplicate> {
    if !config.rule_enabled("duplicate-code") {
        return Vec::new();
    }
    let sources: Vec<(PathBuf, Language, String)> = files
        .iter()
        .filter(|file| !is_ignored(ignore, &file.file))
        .map(|file| (file.file.clone(), file.language, file.source.clone()))
        .collect();
    find_duplicates(
        &sources,
        DuplicateOptions {
            min_tokens: config.duplicate_code_min_tokens,
            compare_markup: config.duplicates_compare_markup,
        },
    )
}

fn is_ignored(ignore: Option<&PathFilter>, path: &Path) -> bool {
    ignore.is_some_and(|filter| filter.matches(path))
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
    use std::sync::atomic::AtomicUsize;

    use nullscan_core::DuplicateLocation;

    use crate::ignore::PathFilter;

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
            overrides: None,
            cancel: None,
            files_ignore: None,
            duplicates_ignore: None,
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

    #[test]
    fn overrides_win_over_the_bytes_on_disk() {
        let root = temp_dir("overrides");
        let clean = root.join("clean.ts");
        let dirty = root.join("dirty.ts");
        fs::write(&clean, "console.log(1);\n").expect("write clean.ts");
        fs::write(&dirty, "const value = 1;\n").expect("write dirty.ts");

        let mut overrides = HashMap::new();
        overrides.insert(clean.clone(), "const value = 1;\n".to_string());
        overrides.insert(dirty.clone(), "console.log(1);\n".to_string());

        let mut config = Config::default();
        config.disabled_rules.insert("duplicate-code".to_string());
        let report = scan(ScanOptions {
            overrides: Some(&overrides),
            ..options(&[root.clone()], config, None)
        });

        let diagnostics = |name: &Path| -> usize {
            report
                .files
                .iter()
                .find(|file| file.file == name)
                .expect("scanned file")
                .diagnostics
                .len()
        };
        assert_eq!(diagnostics(&clean), 0);
        assert!(diagnostics(&dirty) >= 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn cancel_stops_the_scan_and_marks_it_partial() {
        let root = temp_dir("cancel");
        fs::write(root.join("a.ts"), "console.log(1);\n").expect("write a.ts");
        fs::write(root.join("b.ts"), "console.log(2);\n").expect("write b.ts");

        let cancel = AtomicBool::new(true);
        let report = scan(ScanOptions {
            cancel: Some(&cancel),
            ..options(&[root.clone()], Config::default(), None)
        });

        assert!(report.cancelled);
        assert!(report.files.is_empty());
        assert!(report.duplicates.is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn merge_concatenates_counts_and_dedupes_shared_entries() {
        let duplicate = Duplicate {
            token_count: 30,
            a: DuplicateLocation {
                file: PathBuf::from("/repo/a.ts"),
                start_line: 1,
                end_line: 3,
            },
            b: DuplicateLocation {
                file: PathBuf::from("/repo/b.ts"),
                start_line: 5,
                end_line: 7,
            },
        };

        let first = ScanReport {
            files: vec![FileResult {
                file: PathBuf::from("/repo/a.ts"),
                language: Language::TypeScript,
                source: "a\n".to_string(),
                diagnostics: Vec::new(),
            }],
            duplicates: vec![duplicate],
            warnings: vec!["warning: shared".to_string()],
            duration: Duration::from_millis(10),
            truncated: true,
            cancelled: false,
            config_path: Some(PathBuf::from("/repo/nullscan.toml")),
            disabled_rules: vec!["no-var".to_string()],
        };
        let second = ScanReport {
            files: vec![FileResult {
                file: PathBuf::from("/repo/b.ts"),
                language: Language::TypeScript,
                source: "b\n".to_string(),
                diagnostics: Vec::new(),
            }],
            duplicates: vec![Duplicate {
                token_count: 30,
                // Same span pair with the sides swapped still counts as one duplicate.
                a: DuplicateLocation {
                    file: PathBuf::from("/repo/b.ts"),
                    start_line: 5,
                    end_line: 7,
                },
                b: DuplicateLocation {
                    file: PathBuf::from("/repo/a.ts"),
                    start_line: 1,
                    end_line: 3,
                },
            }],
            warnings: vec!["warning: shared".to_string(), "warning: only".to_string()],
            duration: Duration::from_millis(15),
            truncated: false,
            cancelled: true,
            config_path: Some(PathBuf::from("/repo/packages/p/nullscan.toml")),
            disabled_rules: vec!["eqeqeq".to_string()],
        };

        let merged = merge(vec![first, second]);

        assert_eq!(merged.files.len(), 2);
        assert_eq!(merged.duplicates.len(), 1);
        assert_eq!(merged.warnings, vec!["warning: only", "warning: shared"]);
        assert_eq!(merged.disabled_rules, vec!["eqeqeq", "no-var"]);
        assert_eq!(merged.duration, Duration::from_millis(25));
        assert!(merged.truncated);
        assert!(merged.cancelled);
        assert_eq!(merged.config_path, None);
    }

    #[test]
    fn files_ignore_removes_directories_from_the_walk() {
        let root = temp_dir("files-ignore");
        fs::create_dir_all(root.join("src")).expect("create src dir");
        fs::write(root.join("src/a.ts"), "console.log(1);\n").expect("write a.ts");
        fs::create_dir_all(root.join("tests")).expect("create tests dir");
        fs::write(root.join("tests/b.ts"), "console.log(2);\n").expect("write b.ts");
        let (files_ignore, warnings) = PathFilter::new(root.clone(), &["tests".to_string()]);
        assert!(warnings.is_empty());

        let report = scan(ScanOptions {
            files_ignore: Some(&files_ignore),
            ..options(&[root.clone()], Config::default(), None)
        });

        assert_eq!(report.files.len(), 1);
        assert_eq!(report.files[0].file, root.join("src/a.ts"));
        assert_eq!(report.files[0].diagnostics.len(), 1);
        assert!(report.warnings.is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn an_explicitly_ignored_file_produces_a_warning() {
        let root = temp_dir("explicit-ignore");
        let file = root.join("tests/b.ts");
        fs::create_dir_all(root.join("tests")).expect("create tests dir");
        fs::write(&file, "console.log(1);\n").expect("write b.ts");
        let (files_ignore, _) = PathFilter::new(root.clone(), &["tests".to_string()]);

        let report = scan(ScanOptions {
            files_ignore: Some(&files_ignore),
            ..options(&[file.clone()], Config::default(), None)
        });

        assert!(report.files.is_empty());
        assert_eq!(
            report.warnings,
            vec![format!(
                "ignoring {}: excluded by nullscan.toml",
                file.display()
            )]
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn duplicates_ignore_keeps_files_analyzed_but_out_of_clone_detection() {
        let root = temp_dir("duplicates-ignore");
        let source = "export function sum(values: number[]): number {\n  let total = 0;\n  for (const value of values) {\n    total += value;\n  }\n  return total;\n}\n";
        fs::create_dir_all(root.join("tests/mocks")).expect("create mocks dir");
        fs::write(root.join("tests/mocks/a.ts"), source).expect("write a.ts");
        fs::write(root.join("tests/mocks/b.ts"), source).expect("write b.ts");

        let mut detected = Config::default();
        detected.duplicate_code_min_tokens = 5;
        let baseline = scan(options(&[root.clone()], detected, None));
        assert_eq!(baseline.files.len(), 2);
        assert!(!baseline.duplicates.is_empty());

        let (duplicates_ignore, warnings) =
            PathFilter::new(root.clone(), &["tests/mocks/**".to_string()]);
        assert!(warnings.is_empty());
        let mut filter_config = Config::default();
        filter_config.duplicate_code_min_tokens = 5;
        let filtered = scan(ScanOptions {
            duplicates_ignore: Some(&duplicates_ignore),
            ..options(&[root.clone()], filter_config, None)
        });

        assert_eq!(filtered.files.len(), 2);
        assert!(filtered.duplicates.is_empty());
        let _ = fs::remove_dir_all(root);
    }
}
