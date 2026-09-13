//! Plain, `serde`-free config type. `nullscan-core` stays parsing-agnostic — reading
//! and deserializing an actual config file is the CLI's job (or any other consumer's).

use std::collections::HashSet;

pub struct Config {
    pub disabled_rules: HashSet<String>,
    pub cognitive_complexity_threshold: u32,
    pub duplicate_code_min_tokens: usize,
    pub duplicate_string_threshold: usize,
    pub duplicate_string_min_length: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            disabled_rules: HashSet::new(),
            cognitive_complexity_threshold: 15,
            duplicate_code_min_tokens: 30,
            duplicate_string_threshold: 3,
            duplicate_string_min_length: 10,
        }
    }
}

impl Config {
    pub fn rule_enabled(&self, rule: &str) -> bool {
        !self.disabled_rules.contains(rule)
    }
}
