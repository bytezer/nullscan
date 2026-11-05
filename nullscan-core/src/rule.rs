//! Rule metadata and the single registry of every rule nullscan knows about.
//!
//! `RULES` is the source of truth for rule IDs, descriptions, and scope. The core
//! analyzer executes the file-scoped entries, while consumers such as the CLI's SARIF
//! output and config validation read the same list instead of maintaining their own.
//! Adding a rule means adding one implementation module plus one entry here; there is no
//! second place to update.

use tree_sitter::Tree;

use crate::lang::typescript::rules;
use crate::{Config, Diagnostic};

/// Everything a per-file rule needs to inspect one parsed file.
///
/// Deliberately a plain borrow bundle: future cross-rule analysis (for example a scope
/// graph) can be added as another field without changing the rule-runner signature.
pub struct FileContext<'a> {
    pub tree: &'a Tree,
    pub source: &'a str,
    pub config: &'a Config,
}

/// Fixed metadata every rule exposes to the analyzer, SARIF, and config validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuleMeta {
    pub id: &'static str,
    pub description: &'static str,
}

/// Whether a rule runs on each file or needs the whole project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleScope {
    File,
    Project,
}

/// A per-file rule implementation. Rules that need project-wide context use a separate
/// orchestrator and leave this as `None`.
pub type RuleRun = for<'a> fn(&FileContext<'a>) -> Vec<Diagnostic>;

/// One registered rule.
pub struct Rule {
    pub meta: RuleMeta,
    pub scope: RuleScope,
    pub run: Option<RuleRun>,
}

/// Every rule, in diagnostic-emission order for file-scoped rules.
///
/// Order matters: `analyze` executes file rules in this order and existing tests assert
/// exact rule vectors. Register a new rule at the position where its diagnostics should
/// appear.
pub const RULES: &[Rule] = &[
    Rule {
        meta: RuleMeta {
            id: "no-console",
            description: "Unexpected console call",
        },
        scope: RuleScope::File,
        run: Some(rules::no_console::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-debugger",
            description: "Unexpected 'debugger' statement",
        },
        scope: RuleScope::File,
        run: Some(rules::no_debugger::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-var",
            description: "Use 'let' or 'const' instead of 'var'",
        },
        scope: RuleScope::File,
        run: Some(rules::no_var::run),
    },
    Rule {
        meta: RuleMeta {
            id: "eqeqeq",
            description: "Use '==='/'!==' instead of '=='/'!='",
        },
        scope: RuleScope::File,
        run: Some(rules::eqeqeq::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-explicit-any",
            description: "Avoid the explicit 'any' type",
        },
        scope: RuleScope::File,
        run: Some(rules::no_explicit_any::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-empty-catch",
            description: "Empty 'catch' block swallows errors",
        },
        scope: RuleScope::File,
        run: Some(rules::no_empty_catch::run),
    },
    Rule {
        meta: RuleMeta {
            id: "cognitive-complexity",
            description: "Function is too cognitively complex",
        },
        scope: RuleScope::File,
        run: Some(rules::cognitive_complexity::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-duplicate-string",
            description: "String literal duplicated across the file",
        },
        scope: RuleScope::File,
        run: Some(rules::no_duplicate_string::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-identical-conditions",
            description: "Branch condition duplicates an earlier one in the same chain",
        },
        scope: RuleScope::File,
        run: Some(rules::no_identical_conditions::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-redundant-boolean",
            description: "Redundant boolean comparison or double negation",
        },
        scope: RuleScope::File,
        run: Some(rules::no_redundant_boolean::run),
    },
    Rule {
        meta: RuleMeta {
            id: "prefer-immediate-return",
            description: "Variable declared only to be returned immediately",
        },
        scope: RuleScope::File,
        run: Some(rules::prefer_immediate_return::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-collapsible-if",
            description: "Nested 'if' can be merged with the outer condition",
        },
        scope: RuleScope::File,
        run: Some(rules::no_collapsible_if::run),
    },
    // Cross-file rule: orchestrated by the CLI because it needs every source at once,
    // but still registered here so SARIF and config validation have one catalog.
    Rule {
        meta: RuleMeta {
            id: "duplicate-code",
            description: "Copy-pasted block of code",
        },
        scope: RuleScope::Project,
        run: None,
    },
];

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    fn ids() -> Vec<&'static str> {
        RULES.iter().map(|rule| rule.meta.id).collect()
    }

    #[test]
    fn rule_ids_are_unique() {
        let ids = ids();
        let unique: HashSet<_> = ids.iter().copied().collect();
        assert_eq!(unique.len(), ids.len(), "duplicate rule id in RULES");
    }

    #[test]
    fn every_rule_has_metadata() {
        for rule in RULES {
            assert!(!rule.meta.id.is_empty(), "rule id must not be empty");
            assert!(
                !rule.meta.description.trim().is_empty(),
                "rule '{}' has an empty description",
                rule.meta.id
            );
        }
    }

    #[test]
    fn duplicate_code_is_a_project_rule() {
        let rule = RULES
            .iter()
            .find(|rule| rule.meta.id == "duplicate-code")
            .expect("duplicate-code must stay registered");
        assert_eq!(rule.scope, RuleScope::Project);
        assert!(rule.run.is_none(), "project rules have no per-file runner");
    }

    #[test]
    fn existing_rule_ids_are_registered() {
        let ids: HashSet<_> = ids().into_iter().collect();
        for id in [
            "no-console",
            "no-debugger",
            "no-var",
            "eqeqeq",
            "no-explicit-any",
            "no-empty-catch",
            "cognitive-complexity",
            "no-duplicate-string",
            "no-identical-conditions",
            "no-redundant-boolean",
            "prefer-immediate-return",
            "no-collapsible-if",
            "duplicate-code",
        ] {
            assert!(ids.contains(id), "expected rule '{id}' to be registered");
        }
    }

    #[test]
    fn file_rules_have_runners() {
        for rule in RULES.iter().filter(|rule| rule.scope == RuleScope::File) {
            assert!(
                rule.run.is_some(),
                "file rule '{}' has no runner",
                rule.meta.id
            );
        }
    }

    #[test]
    fn file_rule_order_is_stable() {
        let order: Vec<_> = RULES
            .iter()
            .filter(|rule| rule.scope == RuleScope::File)
            .map(|rule| rule.meta.id)
            .collect();
        assert_eq!(
            order,
            vec![
                "no-console",
                "no-debugger",
                "no-var",
                "eqeqeq",
                "no-explicit-any",
                "no-empty-catch",
                "cognitive-complexity",
                "no-duplicate-string",
                "no-identical-conditions",
                "no-redundant-boolean",
                "prefer-immediate-return",
                "no-collapsible-if",
            ]
        );
    }
}
