mod cognitive_complexity;
mod config;
mod duplication;
mod lang {
    pub mod typescript;
}
mod language;

pub use config::Config;
pub use duplication::{find_duplicates, Duplicate, DuplicateLocation};
pub use language::Language;

pub struct Diagnostic {
    pub rule: &'static str,
    pub line: usize,
    pub column: usize,
    pub message: String,
}

/// Parses `source` as `language` and runs every rule for that language not disabled by `config`.
pub fn analyze(source: &str, language: Language, config: &Config) -> Vec<Diagnostic> {
    let tree = language.parse(source).expect("parse failed");
    match language {
        Language::TypeScript | Language::Tsx => lang::typescript::analyze(&tree, source, config),
    }
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
