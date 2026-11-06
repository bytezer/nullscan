//! Editor settings pushed by the client.
//!
//! The client may send the section either nested (`{ "nullscan": { ... } }`, what VS Code
//! produces for `workspace/configuration`) or flat (`{ ... }`), and it may send only the
//! keys it cares about. Unknown keys, missing keys and values of the wrong type never fail
//! the server: they fall back to the current value or to the default.

use serde_json::Value;

/// How much analysis the editor should trigger. Every field has a safe default, so a
/// client that never sends settings still gets the documented behavior.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub analyze_on_open: bool,
    pub analyze_on_save: bool,
    pub analyze_on_type: bool,
    pub include_duplicates: bool,
    pub max_diagnostics: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            analyze_on_open: true,
            analyze_on_save: true,
            analyze_on_type: false,
            include_duplicates: true,
            max_diagnostics: 5000,
        }
    }
}

impl Settings {
    /// Parses one settings payload on top of the defaults.
    pub fn from_value(value: &Value) -> Self {
        let mut settings = Self::default();
        settings.apply(value);
        settings
    }

    /// Applies the keys present in `value` and keeps the current value for every other key,
    /// so a partial payload never resets an unrelated choice.
    ///
    /// A nested `nullscan` object wins over a flat key of the same name, because that is
    /// the shape an editor section produces when both are present.
    pub fn apply(&mut self, value: &Value) {
        self.apply_object(value);
        if let Some(section) = value.get("nullscan") {
            self.apply_object(section);
        }
    }

    fn apply_object(&mut self, object: &Value) {
        if let Some(flag) = object.get("analyzeOnOpen").and_then(Value::as_bool) {
            self.analyze_on_open = flag;
        }
        if let Some(flag) = object.get("analyzeOnSave").and_then(Value::as_bool) {
            self.analyze_on_save = flag;
        }
        if let Some(flag) = object.get("analyzeOnType").and_then(Value::as_bool) {
            self.analyze_on_type = flag;
        }
        if let Some(flag) = object.get("includeDuplicates").and_then(Value::as_bool) {
            self.include_duplicates = flag;
        }
        if let Some(max) = object.get("maxDiagnostics").and_then(Value::as_u64) {
            if let Ok(max) = usize::try_from(max) {
                self.max_diagnostics = max;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::json;

    #[test]
    fn missing_or_empty_settings_keep_the_defaults() {
        let defaults = Settings::default();

        assert!(!defaults.analyze_on_type);
        assert!(defaults.analyze_on_open);
        assert_eq!(Settings::from_value(&Value::Null), defaults);
        assert_eq!(Settings::from_value(&json!({})), defaults);
        assert_eq!(Settings::from_value(&json!({ "nullscan": {} })), defaults);
    }

    #[test]
    fn nested_shape_is_the_editor_section() {
        let settings = Settings::from_value(&json!({
            "nullscan": {
                "analyzeOnOpen": false,
                "analyzeOnSave": false,
                "analyzeOnType": true,
                "includeDuplicates": false,
                "maxDiagnostics": 42
            }
        }));

        assert_eq!(
            settings,
            Settings {
                analyze_on_open: false,
                analyze_on_save: false,
                analyze_on_type: true,
                include_duplicates: false,
                max_diagnostics: 42,
            }
        );
    }

    #[test]
    fn flat_shape_is_accepted_too() {
        let settings = Settings::from_value(&json!({
            "analyzeOnSave": false,
            "maxDiagnostics": 7
        }));

        assert!(!settings.analyze_on_save);
        assert_eq!(settings.max_diagnostics, 7);
        assert!(settings.analyze_on_open);
    }

    #[test]
    fn nested_keys_win_over_flat_keys() {
        let settings = Settings::from_value(&json!({
            "analyzeOnOpen": false,
            "nullscan": { "analyzeOnOpen": true }
        }));

        assert!(settings.analyze_on_open);
    }

    #[test]
    fn a_partial_object_only_changes_the_keys_it_names() {
        let settings = Settings::from_value(&json!({ "nullscan": { "analyzeOnOpen": false } }));

        assert!(!settings.analyze_on_open);
        assert!(settings.analyze_on_save);
        assert!(!settings.analyze_on_type);
        assert!(settings.include_duplicates);
        assert_eq!(settings.max_diagnostics, 5000);
    }

    #[test]
    fn wrong_types_are_ignored_instead_of_failing() {
        let settings = Settings::from_value(&json!({
            "nullscan": {
                "analyzeOnOpen": "no",
                "analyzeOnSave": 1,
                "analyzeOnType": null,
                "includeDuplicates": [],
                "maxDiagnostics": -5
            }
        }));

        assert_eq!(settings, Settings::default());
        assert_eq!(Settings::from_value(&json!("not an object")), Settings::default());
    }

    #[test]
    fn apply_keeps_values_a_partial_update_does_not_mention() {
        let mut settings = Settings {
            analyze_on_open: false,
            max_diagnostics: 10,
            ..Settings::default()
        };

        settings.apply(&json!({ "nullscan": { "analyzeOnType": true } }));

        assert!(!settings.analyze_on_open);
        assert_eq!(settings.max_diagnostics, 10);
        assert!(settings.analyze_on_type);
    }
}
