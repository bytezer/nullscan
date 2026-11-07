mod cognitive_complexity;
mod config;
mod control_flow;
mod duplication;
mod lang {
    pub mod typescript;
}
mod language;
mod rule;
mod scope;
mod suppression;
mod text;

pub use config::Config;
pub use duplication::{
    Duplicate, DuplicateLocation, DuplicateOptions, DuplicateReport, DuplicationStats,
    find_duplicates,
};
pub use language::Language;
pub use rule::{FileContext, Rule, RuleMeta, RuleRun, RuleScope, RULES, severity_of};
pub use scope::{
    Binding, BindingKind, Declarator, Reference, Scope, ScopeGraph, ScopeId, ScopeKind,
};
pub use text::byte_col_to_utf16;

/// How serious a diagnostic is. Ordered from most to least severe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl Severity {
    /// Stable lowercase name, used by text, SARIF, and JSON consumers.
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        }
    }
}

/// One rule violation at a 1-based source range.
///
/// `line`/`column` and `end_line`/`end_column` are 1-based byte columns, matching
/// tree-sitter points. Consumers that speak UTF-16 (LSP, JSON) convert the column with
/// [`byte_col_to_utf16`] against the affected line.
pub struct Diagnostic {
    pub rule: &'static str,
    pub severity: Severity,
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
    pub message: String,
}

/// Parses `source` as `language` and runs every file-scoped rule not disabled by `config`.
///
/// Diagnostics covered by a `nullscan-ignore-next-line` or `nullscan-ignore-line` comment
/// (see the `suppression` module) are dropped before returning.
pub fn analyze(source: &str, language: Language, config: &Config) -> Vec<Diagnostic> {
    let tree = language.parse(source).expect("parse failed");

    // Build the scope graph only when an enabled file-scoped rule actually needs it.
    let needs_scopes = RULES.iter().any(|rule| {
        rule.scope == RuleScope::File
            && rule.requires_scope
            && config.rule_enabled(rule.meta.id)
    });
    let scopes = needs_scopes.then(|| ScopeGraph::build(&tree, source));

    let context = FileContext {
        tree: &tree,
        source,
        config,
        scopes: scopes.as_ref(),
    };

    let mut diagnostics = Vec::new();
    for rule in RULES
        .iter()
        .filter(|rule| rule.scope == RuleScope::File && config.rule_enabled(rule.meta.id))
    {
        if let Some(run) = rule.run {
            diagnostics.extend(run(&context));
        }
    }
    suppression::apply(&tree, source, &mut diagnostics);
    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tsx_parses_jsx_and_runs_typescript_rules() {
        let src = "const x = <div>{a == b}</div>;";
        let rules: Vec<_> = analyze(src, Language::Tsx, &Config::default()).iter().map(|d| d.rule).collect();
        assert_eq!(rules, vec!["eqeqeq"]);
    }

    #[test]
    fn severity_names_are_stable() {
        assert_eq!(Severity::Error.as_str(), "error");
        assert_eq!(Severity::Warning.as_str(), "warning");
        assert_eq!(Severity::Info.as_str(), "info");
        assert!(Severity::Error < Severity::Warning && Severity::Warning < Severity::Info);
    }
}
