//! Inline suppression comments: `nullscan-ignore-next-line` and `nullscan-ignore-line`.
//!
//! Only real `//` and `/* */` comment nodes count, so a marker inside a string, template
//! literal or JSX text never suppresses anything. The marker must be the first token of
//! the comment (after the delimiters and whitespace) and must end on a word boundary,
//! which keeps prose such as `// note: nullscan-ignore-next-line` from matching.
//!
//! `nullscan-ignore-next-line` targets the line immediately after the comment's end line;
//! `nullscan-ignore-line` targets the comment's own end line. A marker without a rule list
//! suppresses every rule; otherwise only the comma- and/or whitespace-separated names are
//! suppressed. Stacked markers on the same target line combine, and "all rules" wins.
//!
//! Suppression is keyed by the diagnostic's 1-based start line. It applies to the per-file
//! diagnostics `analyze` produces; project-scoped rules such as `duplicate-code` are
//! orchestrated elsewhere and are not affected.

use std::collections::{HashMap, HashSet, hash_map::Entry};

use tree_sitter::{Node, Tree};

use crate::Diagnostic;

const NEXT_LINE_MARKER: &str = "nullscan-ignore-next-line";
const LINE_MARKER: &str = "nullscan-ignore-line";

/// Drops every diagnostic covered by an inline suppression comment.
pub(crate) fn apply(tree: &Tree, source: &str, diagnostics: &mut Vec<Diagnostic>) {
    if diagnostics.is_empty() {
        return;
    }
    let suppressions = collect(tree.root_node(), source);
    if suppressions.by_line.is_empty() {
        return;
    }
    diagnostics.retain(|diagnostic| !suppressions.suppresses(diagnostic.line, diagnostic.rule));
}

/// Which line a marker targets, relative to the comment's end line.
enum Marker {
    /// The line immediately after the comment's end line.
    NextLine,
    /// The comment's own end line.
    Line,
}

/// One parsed marker: every rule, or only the named ones.
enum Suppression {
    All,
    Only(HashSet<String>),
}

impl Suppression {
    fn matches(&self, rule: &str) -> bool {
        match self {
            Suppression::All => true,
            Suppression::Only(rules) => rules.contains(rule),
        }
    }

    /// Combines a later marker targeting the same line. "All rules" always wins.
    fn merge(&mut self, other: Suppression) {
        match other {
            Suppression::All => *self = Suppression::All,
            Suppression::Only(other_rules) => match self {
                Suppression::All => {}
                Suppression::Only(rules) => rules.extend(other_rules),
            },
        }
    }
}

#[derive(Default)]
struct Suppressions {
    /// Keyed by 1-based target line.
    by_line: HashMap<usize, Suppression>,
}

impl Suppressions {
    fn insert(&mut self, line: usize, suppression: Suppression) {
        match self.by_line.entry(line) {
            Entry::Occupied(mut entry) => entry.get_mut().merge(suppression),
            Entry::Vacant(entry) => {
                entry.insert(suppression);
            }
        }
    }

    fn suppresses(&self, line: usize, rule: &str) -> bool {
        self.by_line
            .get(&line)
            .is_some_and(|suppression| suppression.matches(rule))
    }
}

/// Walks every node with an explicit stack so deeply nested or malformed input cannot
/// overflow the call stack.
fn collect(root: Node<'_>, source: &str) -> Suppressions {
    let mut suppressions = Suppressions::default();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == "comment" {
            record_comment(node, source, &mut suppressions);
        }
        let mut cursor = node.walk();
        stack.extend(node.children(&mut cursor));
    }
    suppressions
}

fn record_comment(node: Node<'_>, source: &str, suppressions: &mut Suppressions) {
    let Ok(text) = node.utf8_text(source.as_bytes()) else {
        return;
    };
    let Some((marker, suppression)) = parse_comment(text) else {
        return;
    };
    let line = match marker {
        Marker::Line => node.end_position().row + 1,
        Marker::NextLine => node.end_position().row + 2,
    };
    suppressions.insert(line, suppression);
}

fn parse_comment(text: &str) -> Option<(Marker, Suppression)> {
    let body = comment_body(text)?;
    if let Some(rest) = strip_marker(body, NEXT_LINE_MARKER) {
        return Some((Marker::NextLine, parse_rules(rest)));
    }
    let rest = strip_marker(body, LINE_MARKER)?;
    Some((Marker::Line, parse_rules(rest)))
}

/// Body of a `//` or `/* */` comment, with the delimiters and leading whitespace removed.
fn comment_body(text: &str) -> Option<&str> {
    let body = if let Some(rest) = text.strip_prefix("//") {
        rest
    } else if let Some(rest) = text.strip_prefix("/*") {
        rest.strip_suffix("*/").unwrap_or(rest)
    } else {
        return None;
    };
    Some(body.trim_start())
}

/// Remaining text after `marker`, or `None` when the marker is absent or continues into
/// another word (for example `nullscan-ignore-next-lines`).
fn strip_marker<'a>(body: &'a str, marker: &str) -> Option<&'a str> {
    let rest = body.strip_prefix(marker)?;
    match rest.chars().next() {
        None => Some(rest),
        Some(next) if !is_word_char(next) => Some(rest),
        Some(_) => None,
    }
}

fn is_word_char(character: char) -> bool {
    character.is_alphanumeric() || character == '_' || character == '-'
}

fn parse_rules(rest: &str) -> Suppression {
    let rules: HashSet<String> = rest
        .split(|character: char| character == ',' || character.is_whitespace())
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect();
    if rules.is_empty() {
        Suppression::All
    } else {
        Suppression::Only(rules)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Language, Severity};

    fn diagnostic(rule: &'static str, line: usize) -> Diagnostic {
        Diagnostic {
            rule,
            severity: Severity::Info,
            line,
            column: 1,
            end_line: line,
            end_column: 2,
            message: String::new(),
        }
    }

    /// Candidate diagnostics at explicit lines, filtered through [`apply`].
    fn kept(source: &str, candidates: &[(&'static str, usize)]) -> Vec<&'static str> {
        kept_in(Language::TypeScript, source, candidates)
    }

    fn kept_in(
        language: Language,
        source: &str,
        candidates: &[(&'static str, usize)],
    ) -> Vec<&'static str> {
        let tree = language.parse(source).expect("test source must parse");
        let mut diagnostics: Vec<Diagnostic> = candidates
            .iter()
            .map(|(rule, line)| diagnostic(rule, *line))
            .collect();
        apply(&tree, source, &mut diagnostics);
        diagnostics.iter().map(|diagnostic| diagnostic.rule).collect()
    }

    #[test]
    fn next_line_marker_suppresses_all_rules() {
        let source = "// nullscan-ignore-next-line\neval(code);";
        assert!(kept(source, &[("no-eval", 2)]).is_empty());
        assert!(kept(source, &[("no-console", 2)]).is_empty());
    }

    #[test]
    fn rule_list_suppresses_only_the_named_rules() {
        let source = "// nullscan-ignore-next-line no-console, eqeqeq\neval(code);";
        assert_eq!(kept(source, &[("eqeqeq", 2), ("no-eval", 2)]), vec!["no-eval"]);

        let whitespace = "// nullscan-ignore-next-line no-console eqeqeq\neval(code);";
        assert_eq!(
            kept(whitespace, &[("no-console", 2), ("eqeqeq", 2), ("no-eval", 2)]),
            vec!["no-eval"]
        );
    }

    #[test]
    fn line_marker_targets_the_comment_line_only() {
        let source = "eval(code); // nullscan-ignore-line no-eval";
        assert!(kept(source, &[("no-eval", 1)]).is_empty());

        let followed = "eval(code); // nullscan-ignore-line no-eval\neval(other);";
        assert_eq!(kept(followed, &[("no-eval", 2)]), vec!["no-eval"]);

        let own_line = "// nullscan-ignore-line\neval(code);";
        assert_eq!(kept(own_line, &[("no-eval", 2)]), vec!["no-eval"]);
    }

    #[test]
    fn block_comments_are_supported() {
        let next_line = "/* nullscan-ignore-next-line */\neval(code);";
        assert!(kept(next_line, &[("no-eval", 2)]).is_empty());

        let same_line = "eval(code); /* nullscan-ignore-line no-eval */";
        assert!(kept(same_line, &[("no-eval", 1)]).is_empty());
    }

    #[test]
    fn markers_inside_strings_and_templates_are_ignored() {
        let string = "const text = \"// nullscan-ignore-next-line\";\neval(code);";
        assert_eq!(kept(string, &[("no-eval", 2)]), vec!["no-eval"]);

        let template = "const text = `// nullscan-ignore-next-line`;\neval(code);";
        assert_eq!(kept(template, &[("no-eval", 2)]), vec!["no-eval"]);
    }

    #[test]
    fn markers_inside_jsx_text_are_ignored() {
        let source =
            "export function View() { return <div>// nullscan-ignore-next-line</div>; }\neval(code);";
        assert_eq!(
            kept_in(Language::Tsx, source, &[("no-eval", 2)]),
            vec!["no-eval"]
        );
    }

    #[test]
    fn marker_must_start_the_comment_and_end_on_a_word_boundary() {
        let prose = "// note: nullscan-ignore-next-line\neval(code);";
        assert_eq!(kept(prose, &[("no-eval", 2)]), vec!["no-eval"]);

        let suffix = "// nullscan-ignore-next-lines\neval(code);";
        assert_eq!(kept(suffix, &[("no-eval", 2)]), vec!["no-eval"]);
    }

    #[test]
    fn marker_only_applies_to_its_immediate_target_line() {
        let source = "// nullscan-ignore-next-line\n\neval(code);";
        assert_eq!(kept(source, &[("no-eval", 3)]), vec!["no-eval"]);
    }

    #[test]
    fn stacked_markers_on_one_target_line_combine() {
        let source = "/* nullscan-ignore-next-line no-console */ /* nullscan-ignore-next-line eqeqeq */\neval(code);";
        assert!(kept(source, &[("no-console", 2), ("eqeqeq", 2)]).is_empty());
        assert_eq!(kept(source, &[("no-eval", 2)]), vec!["no-eval"]);

        let all_wins = "/* nullscan-ignore-next-line no-console */ /* nullscan-ignore-next-line */\neval(code);";
        assert!(kept(all_wins, &[("no-eval", 2)]).is_empty());
    }

    #[test]
    fn unknown_rule_names_are_inert() {
        let source = "// nullscan-ignore-next-line not-a-rule\neval(code);";
        assert_eq!(kept(source, &[("no-eval", 2)]), vec!["no-eval"]);
    }

    #[test]
    fn files_without_markers_are_unchanged() {
        let source = "// just a comment\nconst value = 1;\neval(code);";
        assert_eq!(
            kept(source, &[("no-eval", 3), ("no-console", 2)]),
            vec!["no-eval", "no-console"]
        );
    }

    #[test]
    fn comments_nested_in_blocks_are_found() {
        let source = "function f() {\n  // nullscan-ignore-next-line\n  eval(code);\n}";
        assert!(kept(source, &[("no-eval", 3)]).is_empty());
    }
}
