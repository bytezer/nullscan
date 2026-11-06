//! Minimal edits to `nullscan.toml` for the disable-rule quick fix.
//!
//! The server never writes the file itself; it hands the new text to the client as a
//! workspace edit. This module only knows how `[rules].disabled` looks and how to add one
//! rule to it while keeping every comment, blank line and unrelated key byte-identical.

use toml_edit::{Array, DocumentMut, Item, Table, TableLike, Value};

/// Adds `rule` to `[rules].disabled`, preserving comments and formatting.
///
/// Empty input becomes a minimal config with the rule disabled, an already disabled rule
/// leaves the document unchanged, and a `rules` or `disabled` entry of the wrong TOML type
/// is reported as an error instead of being overwritten.
pub fn add_disabled_rule(content: &str, rule: &str) -> Result<String, String> {
    let mut document = parse_document(content)?;
    let disabled = disabled_array(rules_table(&mut document)?)?;
    if !is_disabled(disabled, rule) {
        disabled.push(rule);
    }
    Ok(document.to_string())
}

/// A blank file is treated as a new document, so its whitespace does not survive.
fn parse_document(content: &str) -> Result<DocumentMut, String> {
    if content.trim().is_empty() {
        return Ok(DocumentMut::new());
    }
    content
        .parse()
        .map_err(|error| format!("invalid nullscan.toml: {error}"))
}

/// The `[rules]` table, created when the document has none.
fn rules_table(document: &mut DocumentMut) -> Result<&mut dyn TableLike, String> {
    if document.get("rules").is_none() {
        document.insert("rules", Item::Table(Table::new()));
    }
    match document.get_mut("rules") {
        Some(Item::Table(table)) => Ok(table),
        Some(Item::Value(Value::InlineTable(table))) => Ok(table),
        Some(_) => Err("nullscan.toml: `rules` must be a table".to_owned()),
        None => Err("nullscan.toml: could not create the `rules` table".to_owned()),
    }
}

/// The `disabled` array, created when the table has none.
fn disabled_array(rules: &mut dyn TableLike) -> Result<&mut Array, String> {
    if rules.get("disabled").is_none() {
        rules.insert("disabled", Item::Value(Value::Array(Array::new())));
    }
    match rules.get_mut("disabled") {
        Some(Item::Value(Value::Array(array))) => Ok(array),
        Some(_) => Err("nullscan.toml: `rules.disabled` must be an array".to_owned()),
        None => Err("nullscan.toml: could not create `rules.disabled`".to_owned()),
    }
}

fn is_disabled(disabled: &Array, rule: &str) -> bool {
    disabled
        .iter()
        .any(|value| value.as_str() == Some(rule))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_content_becomes_a_minimal_config() {
        let edited = add_disabled_rule("", "no-console").expect("edit empty config");

        assert_eq!(edited, "[rules]\ndisabled = [\"no-console\"]\n");
    }

    #[test]
    fn whitespace_only_content_becomes_a_minimal_config() {
        let edited = add_disabled_rule("\n\n", "no-console").expect("edit blank config");

        assert_eq!(edited, "[rules]\ndisabled = [\"no-console\"]\n");
    }

    #[test]
    fn existing_table_keeps_comments_and_other_keys() {
        let content = "# project config\n\
                       [rules]\n\
                       # keep this comment\n\
                       disabled = [\"no-var\"]\n\
                       \n\
                       [thresholds]\n\
                       cognitive-complexity = 20\n";

        let edited = add_disabled_rule(content, "no-console").expect("edit config");

        assert_eq!(
            edited,
            "# project config\n\
             [rules]\n\
             # keep this comment\n\
             disabled = [\"no-var\", \"no-console\"]\n\
             \n\
             [thresholds]\n\
             cognitive-complexity = 20\n"
        );
    }

    #[test]
    fn appends_to_an_existing_array() {
        let content = "[rules]\ndisabled = [\"no-var\"]\n";

        let edited = add_disabled_rule(content, "eqeqeq").expect("append to array");

        assert_eq!(edited, "[rules]\ndisabled = [\"no-var\", \"eqeqeq\"]\n");
    }

    #[test]
    fn an_already_disabled_rule_leaves_the_document_unchanged() {
        let content = "[rules]\ndisabled = [\"no-var\", \"no-console\"]\n";

        let edited = add_disabled_rule(content, "no-console").expect("idempotent edit");

        assert_eq!(edited, content);
    }

    #[test]
    fn unrelated_sections_stay_untouched_when_rules_is_missing() {
        let content = "[thresholds]\ncognitive-complexity = 20\n";

        let edited = add_disabled_rule(content, "no-console").expect("add rules table");

        assert!(edited.starts_with(content), "kept: {edited}");
        assert!(
            edited.contains("[rules]\ndisabled = [\"no-console\"]"),
            "appended: {edited}"
        );
    }

    #[test]
    fn an_inline_rules_table_is_edited_in_place() {
        let content = "rules = { disabled = [\"no-var\"] }\n";

        let edited = add_disabled_rule(content, "no-console").expect("edit inline table");

        assert_eq!(
            edited,
            "rules = { disabled = [\"no-var\", \"no-console\"] }\n"
        );
    }

    #[test]
    fn a_rules_entry_of_the_wrong_type_is_an_error() {
        let error = add_disabled_rule("rules = 1\n", "no-console").expect_err("wrong rules type");

        assert!(error.contains("rules"), "error: {error}");
    }

    #[test]
    fn a_disabled_entry_of_the_wrong_type_is_an_error() {
        let error = add_disabled_rule("[rules]\ndisabled = \"no-console\"\n", "no-console")
            .expect_err("wrong disabled type");

        assert!(error.contains("rules.disabled"), "error: {error}");
    }

    #[test]
    fn invalid_toml_is_an_error() {
        let error = add_disabled_rule("[rules\n", "no-console").expect_err("invalid toml");

        assert!(error.starts_with("invalid nullscan.toml:"), "error: {error}");
    }
}
