//! Plain, `serde`-free config type. `nullscan-core` stays parsing-agnostic — reading
//! and deserializing an actual config file is the CLI's job (or any other consumer's).

use std::collections::HashSet;

pub struct Config {
    pub disabled_rules: HashSet<String>,
    pub cognitive_complexity_threshold: u32,
    pub duplicate_code_min_tokens: usize,
    /// Whether clone detection compares presentational JSX markup. Off by default, so
    /// only the logic inside `{...}` expressions counts and static markup is ignored.
    pub duplicates_compare_markup: bool,
    pub duplicate_string_threshold: usize,
    pub duplicate_string_min_length: usize,
    /// Raw (undecoded) literal contents that `no-duplicate-string` skips entirely, so
    /// project-wide vocabulary such as CSS keywords never counts toward the threshold.
    /// Matching is exact and case-sensitive; there is no globbing.
    pub duplicate_string_ignore: HashSet<String>,
    /// Maximum number of non-default `case` clauses allowed before `max-switch-cases`.
    pub max_switch_cases: usize,
    /// Minimum number of non-default `case` clauses required before `no-small-switch`.
    pub no_small_switch_min_cases: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            disabled_rules: HashSet::new(),
            cognitive_complexity_threshold: 15,
            duplicate_code_min_tokens: 30,
            duplicates_compare_markup: false,
            duplicate_string_threshold: 3,
            duplicate_string_min_length: 10,
            duplicate_string_ignore: HashSet::new(),
            max_switch_cases: 10,
            no_small_switch_min_cases: 3,
        }
    }
}

impl Config {
    pub fn rule_enabled(&self, rule: &str) -> bool {
        !self.disabled_rules.contains(rule)
    }
}
