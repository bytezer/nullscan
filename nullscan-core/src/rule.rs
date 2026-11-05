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
    Rule {
        meta: RuleMeta {
            id: "no-identical-expressions",
            description: "Identical expression used on both sides of a binary operator",
        },
        scope: RuleScope::File,
        run: Some(rules::no_identical_expressions::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-duplicated-branches",
            description: "Two branches share the same implementation",
        },
        scope: RuleScope::File,
        run: Some(rules::no_duplicated_branches::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-all-duplicated-branches",
            description: "All branches share the same implementation",
        },
        scope: RuleScope::File,
        run: Some(rules::no_all_duplicated_branches::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-useless-catch",
            description: "Catch block only rethrows the caught error",
        },
        scope: RuleScope::File,
        run: Some(rules::no_useless_catch::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-redundant-jump",
            description: "Redundant 'return' or 'continue' at the end of a block",
        },
        scope: RuleScope::File,
        run: Some(rules::no_redundant_jump::run),
    },
    Rule {
        meta: RuleMeta {
            id: "prefer-single-boolean-return",
            description: "Boolean return wrapped in an 'if'/'else'",
        },
        scope: RuleScope::File,
        run: Some(rules::prefer_single_boolean_return::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-nested-switch",
            description: "'switch' statement nested inside another 'switch'",
        },
        scope: RuleScope::File,
        run: Some(rules::no_nested_switch::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-nested-template-literals",
            description: "Template literal nested inside another template literal",
        },
        scope: RuleScope::File,
        run: Some(rules::no_nested_template_literals::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-small-switch",
            description: "'switch' has too few 'case' clauses",
        },
        scope: RuleScope::File,
        run: Some(rules::no_small_switch::run),
    },
    Rule {
        meta: RuleMeta {
            id: "max-switch-cases",
            description: "'switch' has too many 'case' clauses",
        },
        scope: RuleScope::File,
        run: Some(rules::max_switch_cases::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-same-line-conditional",
            description: "Conditional starts on the same line as the previous statement",
        },
        scope: RuleScope::File,
        run: Some(rules::no_same_line_conditional::run),
    },
    Rule {
        meta: RuleMeta {
            id: "non-existent-operator",
            description: "Non-existent operator such as '=+' or '=-'",
        },
        scope: RuleScope::File,
        run: Some(rules::non_existent_operator::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-self-assign",
            description: "Variable assigned to itself",
        },
        scope: RuleScope::File,
        run: Some(rules::no_self_assign::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-constant-condition",
            description: "Constant condition in 'if' or ternary expression",
        },
        scope: RuleScope::File,
        run: Some(rules::no_constant_condition::run),
    },
    Rule {
        meta: RuleMeta {
            id: "use-isnan",
            description: "Comparison with 'NaN' instead of Number.isNaN",
        },
        scope: RuleScope::File,
        run: Some(rules::use_isnan::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-array-constructor",
            description: "Array constructed with the 'Array' constructor",
        },
        scope: RuleScope::File,
        run: Some(rules::no_array_constructor::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-new-wrappers",
            description: "Primitive wrapper object created with 'new'",
        },
        scope: RuleScope::File,
        run: Some(rules::no_new_wrappers::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-duplicate-imports",
            description: "Module imported more than once",
        },
        scope: RuleScope::File,
        run: Some(rules::no_duplicate_imports::run),
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
            "no-identical-expressions",
            "no-duplicated-branches",
            "no-all-duplicated-branches",
            "no-useless-catch",
            "no-redundant-jump",
            "prefer-single-boolean-return",
            "no-nested-switch",
            "no-nested-template-literals",
            "no-small-switch",
            "max-switch-cases",
            "no-same-line-conditional",
            "non-existent-operator",
            "no-self-assign",
            "no-constant-condition",
            "use-isnan",
            "no-array-constructor",
            "no-new-wrappers",
            "no-duplicate-imports",
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
                "no-identical-expressions",
                "no-duplicated-branches",
                "no-all-duplicated-branches",
                "no-useless-catch",
                "no-redundant-jump",
                "prefer-single-boolean-return",
                "no-nested-switch",
                "no-nested-template-literals",
                "no-small-switch",
                "max-switch-cases",
                "no-same-line-conditional",
                "non-existent-operator",
                "no-self-assign",
                "no-constant-condition",
                "use-isnan",
                "no-array-constructor",
                "no-new-wrappers",
                "no-duplicate-imports",
            ]
        );
    }
}
