//! Reads and merges `nullscan.toml` into a `nullscan_core::Config`.
//!
//! The core crate's `Config` stays plain and `serde`-free; the TOML shape and its
//! deserialization are entirely a workspace concern. Keeping the loader here means the
//! CLI and the editor integration accept exactly the same file.

use std::fs;
use std::path::{Path, PathBuf};

use nullscan_core::Config;
use serde::Deserialize;

use crate::ignore::PathFilter;

#[derive(Deserialize, Default)]
struct FileConfig {
    rules: Option<FileRulesConfig>,
    thresholds: Option<FileThresholdsConfig>,
    files: Option<FileIgnoreConfig>,
    duplicates: Option<FileDuplicatesConfig>,
}

#[derive(Deserialize, Default)]
struct FileRulesConfig {
    disabled: Option<Vec<String>>,
}

#[derive(Deserialize, Default)]
struct FileIgnoreConfig {
    ignore: Option<Vec<String>>,
}

#[derive(Deserialize, Default)]
struct FileDuplicatesConfig {
    ignore: Option<Vec<String>>,
    #[serde(rename = "compare-markup")]
    compare_markup: Option<bool>,
}

#[derive(Deserialize, Default)]
struct FileThresholdsConfig {
    #[serde(rename = "cognitive-complexity")]
    cognitive_complexity: Option<u32>,
    #[serde(rename = "duplicate-code-min-tokens")]
    duplicate_code_min_tokens: Option<usize>,
    #[serde(rename = "duplicate-string-threshold")]
    duplicate_string_threshold: Option<usize>,
    #[serde(rename = "duplicate-string-min-length")]
    duplicate_string_min_length: Option<usize>,
    #[serde(rename = "max-switch-cases")]
    max_switch_cases: Option<usize>,
    #[serde(rename = "no-small-switch-min-cases")]
    no_small_switch_min_cases: Option<usize>,
}

/// A loaded config plus where it came from and any warnings the file produced.
pub struct LoadedConfig {
    pub config: Config,
    /// Absolute path of the loaded file, or `None` when the built-in defaults are used.
    pub path: Option<PathBuf>,
    /// Non-fatal problems (for example an unknown disabled rule id).
    pub warnings: Vec<String>,
    /// `[files].ignore`, applied to every analysis.
    pub ignore: PathFilter,
    /// `[duplicates].ignore`, applied only to clone detection.
    pub duplicates_ignore: PathFilter,
    /// Directory ignore patterns are relative to.
    pub root: PathBuf,
}

/// Resolves and loads the config: `explicit_path` if given, else `./nullscan.toml` if it
/// exists, else the built-in defaults with no file read at all.
///
/// Returns `Err` with a message ready to print on a missing explicit path or a TOML
/// parse error; callers decide how to fail. No input from the user can panic this code.
pub fn load(explicit_path: Option<&Path>) -> Result<LoadedConfig, String> {
    match resolve_path(explicit_path) {
        Some(path) => load_file(&path),
        None => Ok(defaults_in(&current_dir())),
    }
}

/// Loads `dir/nullscan.toml` when it exists, else the built-in defaults rooted at `dir`.
///
/// This is the editor-facing entry point: every workspace folder resolves its own config
/// without depending on the process working directory. A missing file is not an error.
pub fn load_in_dir(dir: &Path) -> Result<LoadedConfig, String> {
    let candidate = dir.join("nullscan.toml");
    if candidate.is_file() {
        return load_file(&candidate);
    }

    Ok(defaults_in(dir))
}

fn load_file(path: &Path) -> Result<LoadedConfig, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    let file_config: FileConfig = toml::from_str(&text)
        .map_err(|error| format!("invalid config file {}: {error}", path.display()))?;

    let root = config_root(path);
    let mut config = Config::default();
    let mut warnings = Vec::new();
    apply_rules(&file_config, &mut config, path, &mut warnings);
    apply_thresholds(&file_config, &mut config);
    apply_duplicates(&file_config, &mut config);

    let ignore_patterns = file_config
        .files
        .as_ref()
        .and_then(|files| files.ignore.as_deref())
        .unwrap_or_default();
    let duplicates_patterns = file_config
        .duplicates
        .as_ref()
        .and_then(|duplicates| duplicates.ignore.as_deref())
        .unwrap_or_default();
    let (ignore, mut ignore_warnings) = PathFilter::new(root.clone(), ignore_patterns);
    let (duplicates_ignore, mut duplicates_warnings) =
        PathFilter::new(root.clone(), duplicates_patterns);
    warnings.append(&mut ignore_warnings);
    warnings.append(&mut duplicates_warnings);

    Ok(LoadedConfig {
        config,
        path: Some(absolute(path.to_path_buf())),
        warnings,
        ignore,
        duplicates_ignore,
        root,
    })
}

/// Built-in defaults with empty filters rooted at `root`.
///
/// Used when a config file is missing or broken but callers still need the root to apply
/// the process-wide defaults.
pub fn defaults_in(root: &Path) -> LoadedConfig {
    let root = absolute(root.to_path_buf());
    let (ignore, _) = PathFilter::new(root.clone(), &[]);
    let (duplicates_ignore, _) = PathFilter::new(root.clone(), &[]);
    LoadedConfig {
        config: Config::default(),
        path: None,
        warnings: Vec::new(),
        ignore,
        duplicates_ignore,
        root,
    }
}

fn config_root(path: &Path) -> PathBuf {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map_or_else(current_dir, |parent| absolute(parent.to_path_buf()))
}

fn current_dir() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn resolve_path(explicit_path: Option<&Path>) -> Option<PathBuf> {
    match explicit_path {
        Some(path) => Some(path.to_path_buf()),
        None if Path::new("nullscan.toml").is_file() => Some(PathBuf::from("nullscan.toml")),
        None => None,
    }
}

fn apply_rules(
    file_config: &FileConfig,
    config: &mut Config,
    path: &Path,
    warnings: &mut Vec<String>,
) {
    let Some(rules) = &file_config.rules else {
        return;
    };
    let Some(disabled) = &rules.disabled else {
        return;
    };

    let known_ids: Vec<&str> = nullscan_core::RULES.iter().map(|rule| rule.meta.id).collect();
    for name in disabled {
        if !known_ids.contains(&name.as_str()) {
            warnings.push(format!(
                "warning: {} disables unknown rule '{name}'",
                path.display()
            ));
        }
    }
    config.disabled_rules = disabled.iter().cloned().collect();
}

fn apply_thresholds(file_config: &FileConfig, config: &mut Config) {
    let Some(thresholds) = &file_config.thresholds else {
        return;
    };
    if let Some(value) = thresholds.cognitive_complexity {
        config.cognitive_complexity_threshold = value;
    }
    if let Some(value) = thresholds.duplicate_code_min_tokens {
        config.duplicate_code_min_tokens = value;
    }
    if let Some(value) = thresholds.duplicate_string_threshold {
        config.duplicate_string_threshold = value;
    }
    if let Some(value) = thresholds.duplicate_string_min_length {
        config.duplicate_string_min_length = value;
    }
    if let Some(value) = thresholds.max_switch_cases {
        config.max_switch_cases = value;
    }
    if let Some(value) = thresholds.no_small_switch_min_cases {
        config.no_small_switch_min_cases = value;
    }
}

fn apply_duplicates(file_config: &FileConfig, config: &mut Config) {
    let Some(duplicates) = &file_config.duplicates else {
        return;
    };
    if let Some(value) = duplicates.compare_markup {
        config.duplicates_compare_markup = value;
    }
}

fn absolute(path: PathBuf) -> PathBuf {
    fs::canonicalize(&path)
        .or_else(|_| std::path::absolute(&path))
        .unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::process;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn temp_file(label: &str, contents: &str) -> PathBuf {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "nullscan-config-{label}-{}-{id}",
            process::id()
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("nullscan.toml");
        fs::write(&path, contents).expect("write config");
        path
    }

    fn temp_dir(label: &str) -> PathBuf {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "nullscan-config-dir-{label}-{}-{id}",
            process::id()
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn defaults_when_no_file_is_loaded() {
        let loaded = load(None).expect("defaults load");

        assert!(loaded.path.is_none());
        assert!(loaded.warnings.is_empty());
        assert!(loaded.config.disabled_rules.is_empty());
        assert_eq!(loaded.config.cognitive_complexity_threshold, 15);
        assert_eq!(loaded.config.duplicate_code_min_tokens, 30);
        assert!(loaded.ignore.is_empty());
        assert!(loaded.duplicates_ignore.is_empty());
        assert_eq!(loaded.root, std::env::current_dir().expect("cwd"));
    }

    #[test]
    fn explicit_file_overrides_rules_and_thresholds() {
        let path = temp_file(
            "explicit",
            r#"
            [rules]
            disabled = ["no-console", "no-var"]

            [thresholds]
            cognitive-complexity = 25
            max-switch-cases = 7
            "#,
        );

        let loaded = load(Some(&path)).expect("load explicit config");

        assert_eq!(loaded.path, Some(path.canonicalize().expect("canonicalize")));
        assert!(loaded.warnings.is_empty());
        assert!(loaded.config.rule_enabled("eqeqeq"));
        assert!(!loaded.config.rule_enabled("no-console"));
        assert!(!loaded.config.rule_enabled("no-var"));
        assert_eq!(loaded.config.cognitive_complexity_threshold, 25);
        assert_eq!(loaded.config.max_switch_cases, 7);
        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn unknown_disabled_rule_is_a_warning() {
        let path = temp_file(
            "unknown",
            r#"
            [rules]
            disabled = ["not-a-rule"]
            "#,
        );

        let loaded = load(Some(&path)).expect("load config");

        assert_eq!(loaded.warnings.len(), 1);
        assert!(loaded.warnings[0].contains("disables unknown rule 'not-a-rule'"));
        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn parse_error_mentions_the_file() {
        let path = temp_file("broken", "this is not = toml [");

        let Err(error) = load(Some(&path)) else {
            panic!("broken config must fail");
        };

        assert!(error.starts_with(&format!("invalid config file {}:", path.display())));
        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn missing_explicit_file_fails() {
        let path = std::env::temp_dir().join("nullscan-config-does-not-exist.toml");

        let Err(error) = load(Some(&path)) else {
            panic!("missing config must fail");
        };

        assert!(error.starts_with(&format!("could not read {}:", path.display())));
    }

    #[test]
    fn load_in_dir_uses_the_folder_file() {
        let dir = temp_dir("workspace");
        fs::write(
            dir.join("nullscan.toml"),
            "[rules]\ndisabled = [\"no-console\"]\n",
        )
        .expect("write config");

        let loaded = load_in_dir(&dir).expect("load folder config");

        assert_eq!(
            loaded.path,
            Some(dir.join("nullscan.toml").canonicalize().expect("canonicalize"))
        );
        assert!(!loaded.config.rule_enabled("no-console"));
        assert!(loaded.warnings.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_in_dir_without_file_returns_defaults() {
        let dir = temp_dir("empty");

        let loaded = load_in_dir(&dir).expect("defaults load");

        assert!(loaded.path.is_none());
        assert!(loaded.warnings.is_empty());
        assert!(loaded.config.rule_enabled("no-console"));
        assert_eq!(loaded.config.cognitive_complexity_threshold, 15);
        assert!(loaded.ignore.is_empty());
        assert!(loaded.duplicates_ignore.is_empty());
        assert_eq!(loaded.root, dir.canonicalize().expect("canonicalize"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn both_ignore_sections_are_parsed() {
        let path = temp_file(
            "ignore-sections",
            r#"
            [files]
            ignore = ["tests", "**/*.test.ts"]

            [duplicates]
            ignore = ["tests/mocks/**"]
            "#,
        );
        let root = path.parent().expect("parent").to_path_buf();

        let loaded = load(Some(&path)).expect("load config");

        assert!(loaded.warnings.is_empty());
        assert!(!loaded.ignore.is_empty());
        assert!(loaded.ignore.matches(&root.join("src/tests/a.ts")));
        assert!(loaded.ignore.matches(&root.join("src/deep/a.test.ts")));
        assert!(!loaded.ignore.matches(&root.join("src/a.ts")));
        assert!(!loaded.duplicates_ignore.is_empty());
        assert!(loaded.duplicates_ignore.matches(&root.join("tests/mocks/a.ts")));
        assert!(!loaded.duplicates_ignore.matches(&root.join("tests/a.ts")));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn absent_ignore_sections_mean_no_filtering() {
        let path = temp_file("ignore-absent", "[rules]\ndisabled = []\n");

        let loaded = load(Some(&path)).expect("load config");

        assert!(loaded.ignore.is_empty());
        assert!(loaded.duplicates_ignore.is_empty());
        assert!(!loaded.ignore.matches(&path));
        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn compare_markup_defaults_to_false() {
        let path = temp_file("compare-markup-default", "[duplicates]\nignore = []\n");

        let loaded = load(Some(&path)).expect("load config");

        assert!(!loaded.config.duplicates_compare_markup);
        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn compare_markup_can_be_enabled() {
        let path = temp_file("compare-markup-true", "[duplicates]\ncompare-markup = true\n");

        let loaded = load(Some(&path)).expect("load config");

        assert!(loaded.config.duplicates_compare_markup);
        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn compare_markup_can_be_disabled_explicitly() {
        let path = temp_file("compare-markup-false", "[duplicates]\ncompare-markup = false\n");

        let loaded = load(Some(&path)).expect("load config");

        assert!(!loaded.config.duplicates_compare_markup);
        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn wrong_compare_markup_type_is_a_config_error() {
        let path = temp_file(
            "compare-markup-wrong-type",
            "[duplicates]\ncompare-markup = \"yes\"\n",
        );

        let Err(error) = load(Some(&path)) else {
            panic!("wrong compare-markup type must fail");
        };

        assert!(error.starts_with(&format!("invalid config file {}:", path.display())));
        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn duplicate_ignore_and_compare_markup_coexist() {
        let path = temp_file(
            "duplicates-coexist",
            r#"
            [duplicates]
            ignore = ["tests/mocks/**"]
            compare-markup = true
            "#,
        );
        let root = path.parent().expect("parent").to_path_buf();

        let loaded = load(Some(&path)).expect("load config");

        assert!(loaded.warnings.is_empty());
        assert!(loaded.config.duplicates_compare_markup);
        assert!(loaded.duplicates_ignore.matches(&root.join("tests/mocks/a.ts")));
        assert!(!loaded.duplicates_ignore.matches(&root.join("tests/a.ts")));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn wrong_ignore_type_is_a_config_error() {
        let path = temp_file("ignore-wrong-type", "[files]\nignore = \"tests\"\n");

        let Err(error) = load(Some(&path)) else {
            panic!("wrong ignore type must fail");
        };

        assert!(error.starts_with(&format!("invalid config file {}:", path.display())));
        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn ignore_warnings_are_merged_into_the_loaded_config() {
        let path = temp_file("ignore-warning", "[files]\nignore = [\"!tests\"]\n");

        let loaded = load(Some(&path)).expect("load config");

        assert_eq!(loaded.warnings.len(), 1);
        assert!(loaded.warnings[0].contains("negation is not supported"));
        assert!(loaded.ignore.is_empty());
        let _ = fs::remove_dir_all(path.parent().expect("parent"));
    }

    #[test]
    fn root_is_the_directory_that_contains_the_config() {
        let dir = temp_dir("root");
        let path = dir.join("nullscan.toml");
        fs::write(&path, "[files]\nignore = [\"tests\"]\n").expect("write config");

        let explicit = load(Some(&path)).expect("load explicit config");
        assert_eq!(explicit.root, dir.canonicalize().expect("canonicalize"));
        assert!(explicit.ignore.matches(&dir.join("tests/a.ts")));

        let folder = load_in_dir(&dir).expect("load folder config");
        assert_eq!(folder.root, dir.canonicalize().expect("canonicalize"));
        let _ = fs::remove_dir_all(&dir);
    }
}
