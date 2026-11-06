//! Reads and merges `nullscan.toml` into a `nullscan_core::Config`.
//!
//! The core crate's `Config` stays plain and `serde`-free; the TOML shape and its
//! deserialization are entirely a workspace concern. Keeping the loader here means the
//! CLI and the editor integration accept exactly the same file.

use std::fs;
use std::path::{Path, PathBuf};

use nullscan_core::Config;
use serde::Deserialize;

#[derive(Deserialize, Default)]
struct FileConfig {
    rules: Option<FileRulesConfig>,
    thresholds: Option<FileThresholdsConfig>,
}

#[derive(Deserialize, Default)]
struct FileRulesConfig {
    disabled: Option<Vec<String>>,
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
}

/// Resolves and loads the config: `explicit_path` if given, else `./nullscan.toml` if it
/// exists, else the built-in defaults with no file read at all.
///
/// Returns `Err` with a message ready to print on a missing explicit path or a TOML
/// parse error; callers decide how to fail. No input from the user can panic this code.
pub fn load(explicit_path: Option<&Path>) -> Result<LoadedConfig, String> {
    let path = resolve_path(explicit_path);
    let Some(path) = path else {
        return Ok(LoadedConfig {
            config: Config::default(),
            path: None,
            warnings: Vec::new(),
        });
    };

    let text = fs::read_to_string(&path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    let file_config: FileConfig = toml::from_str(&text)
        .map_err(|error| format!("invalid config file {}: {error}", path.display()))?;

    let mut config = Config::default();
    let mut warnings = Vec::new();
    apply_rules(&file_config, &mut config, &path, &mut warnings);
    apply_thresholds(&file_config, &mut config);

    Ok(LoadedConfig {
        config,
        path: Some(absolute(path)),
        warnings,
    })
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

    #[test]
    fn defaults_when_no_file_is_loaded() {
        let loaded = load(None).expect("defaults load");

        assert!(loaded.path.is_none());
        assert!(loaded.warnings.is_empty());
        assert!(loaded.config.disabled_rules.is_empty());
        assert_eq!(loaded.config.cognitive_complexity_threshold, 15);
        assert_eq!(loaded.config.duplicate_code_min_tokens, 30);
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
}
