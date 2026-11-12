//! Flags string literals repeated `Config::duplicate_string_threshold` times in the same
//! file (mirrors SonarJS S1192); short strings are exempt since they're usually
//! incidental (e.g. `""`, `"id"`), not copy-pasted business text worth extracting into a
//! constant.
//!
//! Literals listed in `Config::duplicate_string_ignore` are dropped before counting. The
//! comparison uses the raw content with the surrounding quotes stripped and no escape
//! decoding, so entries must be written the way they appear in the source.
//!
//! Like SonarJS, a string that is directly a JSX attribute value (`values="0 0 1"`) is not
//! counted: markup such as exported SVGs repeats those values by design. A string inside
//! an expression container (`attr={"x"}`) is still code and still counts.

use std::collections::HashMap;

use tree_sitter::{Node, Query, QueryCursor, StreamingIterator};

use crate::Diagnostic;
use crate::rule::FileContext;

use super::{node_text, report_at, static_string_text};

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let threshold = ctx.config.duplicate_string_threshold;
    let mut occurrences = relevant_occurrences(ctx);
    occurrences.sort_by_key(|node| node.start_byte());

    let mut counts: HashMap<&str, usize> = HashMap::new();
    for node in &occurrences {
        *counts.entry(node_text(*node, ctx.source)).or_insert(0) += 1;
    }

    let mut seen_so_far: HashMap<&str, usize> = HashMap::new();
    let mut out = Vec::new();
    for node in &occurrences {
        let text = node_text(*node, ctx.source);
        let total = counts[text];
        if total < threshold {
            continue;
        }
        let index = seen_so_far.entry(text).or_insert(0);
        *index += 1;
        if *index >= threshold {
            out.push(report_at(
                "no-duplicate-string",
                *node,
                format!(
                    "string literal {text} is duplicated {total} times, consider extracting a constant"
                ),
            ));
        }
    }
    out
}

/// Every string literal worth counting: long enough to matter and not on the ignore list.
fn relevant_occurrences<'tree, 'src>(ctx: &'tree FileContext<'src>) -> Vec<Node<'tree>> {
    let min_len = ctx.config.duplicate_string_min_length;
    let ignored = &ctx.config.duplicate_string_ignore;
    let query = Query::new(&ctx.tree.language(), "(string) @report")
        .expect("invalid no-duplicate-string query");
    let report_idx = query.capture_index_for_name("report").unwrap();

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, ctx.tree.root_node(), ctx.source.as_bytes());
    let mut occurrences = Vec::new();
    while let Some(m) = matches.next() {
        let capture = m.captures().iter().find(|c| c.index == report_idx).unwrap();
        // Measure and match the unquoted content; escape sequences stay encoded on both
        // sides, so an ignore entry is compared literally.
        if is_jsx_attribute_value(capture.node) {
            continue;
        }
        let Some(content) = static_string_text(capture.node, ctx.source) else {
            continue;
        };
        if content.len() >= min_len && !ignored.contains(content) {
            occurrences.push(capture.node);
        }
    }
    occurrences
}

fn is_jsx_attribute_value(node: Node<'_>) -> bool {
    node.parent().is_some_and(|parent| parent.kind() == "jsx_attribute")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Config, Language};

    fn config_ignoring(entries: &[&str]) -> Config {
        let mut config = Config::default();
        config.duplicate_string_ignore = entries.iter().map(|entry| (*entry).to_string()).collect();
        config
    }

    fn findings(source: &str, config: &Config) -> Vec<Diagnostic> {
        findings_in(source, Language::TypeScript, config)
    }

    fn findings_in(source: &str, language: Language, config: &Config) -> Vec<Diagnostic> {
        crate::analyze(source, language, config)
            .into_iter()
            .filter(|diagnostic| diagnostic.rule == "no-duplicate-string")
            .collect()
    }

    #[test]
    fn ignored_literals_are_not_counted() {
        let source = r#"
            const shades = [
                "transparent",
                "transparent",
                "transparent",
                "transparent",
                "transparent",
            ];
            export { shades };
        "#;

        assert!(findings(source, &config_ignoring(&["transparent"])).is_empty());
    }

    #[test]
    fn other_duplicated_literals_still_report() {
        let source = r#"
            const shades = [
                "transparent",
                "transparent",
                "transparent",
                "transparent",
                "transparent",
                "currentColor",
                "currentColor",
                "currentColor",
            ];
            export { shades };
        "#;

        let config = config_ignoring(&["transparent"]);
        let diagnostics = findings(source, &config);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("\"currentColor\""));
    }

    #[test]
    fn ignoring_is_exact_and_case_sensitive() {
        let source = r#"
            const shades = [
                "Transparent",
                "Transparent",
                "Transparent",
                "Transparent",
                "Transparent",
            ];
            export { shades };
        "#;

        let diagnostics = findings(source, &config_ignoring(&["transparent"]));

        assert_eq!(diagnostics.len(), 3);
        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| diagnostic.message.contains("\"Transparent\""))
        );
    }

    #[test]
    fn ignore_matches_without_the_surrounding_quotes() {
        let source = r#"
            const shades = [
                'transparent',
                'transparent',
                'transparent',
                'transparent',
                'transparent',
            ];
            export { shades };
        "#;

        assert!(findings(source, &config_ignoring(&["transparent"])).is_empty());
    }

    #[test]
    fn jsx_attribute_values_are_not_counted() {
        let source = r#"
            export const Icon = (
                <svg>
                    <feColorMatrix values="0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 127 0" />
                    <feColorMatrix values="0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 127 0" />
                    <feColorMatrix values="0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 127 0" />
                    <feColorMatrix values="0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 127 0" />
                </svg>
            );
        "#;

        assert!(findings_in(source, Language::Tsx, &Config::default()).is_empty());
    }

    #[test]
    fn strings_in_jsx_expressions_still_count() {
        let source = r#"
            export const Icon = (
                <svg>
                    <path d={"M0 0 L10 10 Z repeated"} />
                    <path d={"M0 0 L10 10 Z repeated"} />
                    <path d={"M0 0 L10 10 Z repeated"} />
                    <path d={"M0 0 L10 10 Z repeated"} />
                </svg>
            );
        "#;

        assert!(!findings_in(source, Language::Tsx, &Config::default()).is_empty());
    }
}
