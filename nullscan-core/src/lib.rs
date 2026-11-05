mod cognitive_complexity;
mod config;
mod duplication;
mod lang {
    pub mod typescript;
}
mod language;
mod rule;

pub use config::Config;
pub use duplication::{find_duplicates, Duplicate, DuplicateLocation};
pub use language::Language;
pub use rule::{FileContext, Rule, RuleMeta, RuleRun, RuleScope, RULES};

pub struct Diagnostic {
    pub rule: &'static str,
    pub line: usize,
    pub column: usize,
    pub message: String,
}

/// Parses `source` as `language` and runs every file-scoped rule not disabled by `config`.
pub fn analyze(source: &str, language: Language, config: &Config) -> Vec<Diagnostic> {
    let tree = language.parse(source).expect("parse failed");
    let context = FileContext {
        tree: &tree,
        source,
        config,
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
}
