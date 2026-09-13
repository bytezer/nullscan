//! Reads and merges `nullscan.toml` into a `nullscan_core::Config`. The core crate's
//! `Config` stays plain and `serde`-free (see its doc comment); the TOML shape and its
//! deserialization are entirely this CLI's concern.

use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::sarif::RULE_DESCRIPTIONS;

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
}

/// Resolves and loads the config: `explicit_path` if given, else `./nullscan.toml` if
/// it exists, else the built-in defaults with no file read at all.
///
/// Returns `Err` with a message ready to print on a missing explicit path or a TOML
/// parse error; the caller decides how to fail (this crate doesn't panic on bad input
/// from the user, only on our own invariants).
pub fn load(explicit_path: Option<&str>) -> Result<nullscan_core::Config, String> {
    let path = match explicit_path {
        Some(p) => Some(p.to_string()),
        None if Path::new("nullscan.toml").is_file() => Some("nullscan.toml".to_string()),
        None => None,
    };

    let Some(path) = path else {
        return Ok(nullscan_core::Config::default());
    };

    let text = fs::read_to_string(&path).map_err(|e| format!("could not read {path}: {e}"))?;
    let file_config: FileConfig =
        toml::from_str(&text).map_err(|e| format!("invalid config file {path}: {e}"))?;

    let mut config = nullscan_core::Config::default();

    if let Some(rules) = file_config.rules {
        if let Some(disabled) = rules.disabled {
            let known_ids: Vec<&str> = RULE_DESCRIPTIONS.iter().map(|(id, _)| *id).collect();
            for name in &disabled {
                if !known_ids.contains(&name.as_str()) {
                    eprintln!("warning: {path} disables unknown rule '{name}'");
                }
            }
            config.disabled_rules = disabled.into_iter().collect();
        }
    }

    if let Some(thresholds) = file_config.thresholds {
        if let Some(v) = thresholds.cognitive_complexity {
            config.cognitive_complexity_threshold = v;
        }
        if let Some(v) = thresholds.duplicate_code_min_tokens {
            config.duplicate_code_min_tokens = v;
        }
        if let Some(v) = thresholds.duplicate_string_threshold {
            config.duplicate_string_threshold = v;
        }
        if let Some(v) = thresholds.duplicate_string_min_length {
            config.duplicate_string_min_length = v;
        }
    }

    Ok(config)
}
