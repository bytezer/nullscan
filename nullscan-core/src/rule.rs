//! Rule metadata and the single registry of every rule nullscan knows about.
//!
//! `RULES` is the source of truth for rule IDs, descriptions, human-facing titles,
//! plain-language explanations, and scope. The core
//! analyzer executes the file-scoped entries, while consumers such as the CLI's SARIF
//! output and config validation read the same list instead of maintaining their own.
//! Adding a rule means adding one implementation module plus one entry here; there is no
//! second place to update.

use tree_sitter::Tree;

use crate::lang::typescript::rules;
use crate::scope::ScopeGraph;
use crate::{Config, Diagnostic, Severity};

/// Everything a per-file rule needs to inspect one parsed file.
///
/// Deliberately a plain borrow bundle: future cross-rule analysis (for example a scope
/// graph) can be added as another field without changing the rule-runner signature.
pub struct FileContext<'a> {
    pub tree: &'a Tree,
    pub source: &'a str,
    pub config: &'a Config,
    /// Present only when at least one enabled file rule declares `requires_scope`.
    pub scopes: Option<&'a ScopeGraph>,
}

/// Fixed metadata every rule exposes to the analyzer, SARIF, and config validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuleMeta {
    pub id: &'static str,
    /// What the rule detects, phrased for a rule catalog.
    pub description: &'static str,
    /// Short human name, sentence case.
    pub title: &'static str,
    /// One plain-language sentence on the consequence and, when natural, the fix.
    pub why: &'static str,
    /// Report level, derived from the rule's catalog category.
    pub severity: Severity,
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
    /// Whether the rule needs the per-file [`ScopeGraph`]. The analyzer builds it lazily,
    /// only when at least one enabled rule sets this to `true`.
    pub requires_scope: bool,
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
            title: "Remove console calls",
            why: "Console calls are for local debugging only; remove them or log through the project logger.",
            severity: Severity::Info,
        },
        scope: RuleScope::File,
        requires_scope: false,
        run: Some(rules::no_console::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-debugger",
            description: "Unexpected 'debugger' statement",
            title: "Remove debugger statements",
            why: "'debugger' pauses execution in open dev tools and can freeze a user's browser; remove it.",
            severity: Severity::Info,
        },
        scope: RuleScope::File,
        requires_scope: false,
        run: Some(rules::no_debugger::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-var",
            description: "Use 'let' or 'const' instead of 'var'",
            title: "Use let or const",
            why: "'var' is function-scoped and can be redeclared, which hides mistakes; use 'let' or 'const'.",
            severity: Severity::Info,
        },
        scope: RuleScope::File,
        requires_scope: false,
        run: Some(rules::no_var::run),
    },
    Rule {
        meta: RuleMeta {
            id: "eqeqeq",
            description: "Use '==='/'!==' instead of '=='/'!='",
            title: "Use strict equality",
            why: "'==' converts types first, so 0 == '' is true; '===' compares type and value.",
            severity: Severity::Warning,
        },
        scope: RuleScope::File,
        requires_scope: false,
        run: Some(rules::eqeqeq::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-explicit-any",
            description: "Avoid the explicit 'any' type",
            title: "Avoid the any type",
            why: "'any' turns off type checking for a value; use a real type or 'unknown' and narrow it.",
            severity: Severity::Info,
        },
        scope: RuleScope::File,
        requires_scope: false,
        run: Some(rules::no_explicit_any::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-empty-catch",
            description: "Empty 'catch' block swallows errors",
            title: "Handle caught errors",
            why: "An empty catch hides failures, so bugs go unnoticed; handle the error or explain why it is safe.",
            severity: Severity::Warning,
        },
        scope: RuleScope::File,
        requires_scope: false,
        run: Some(rules::no_empty_catch::run),
    },
    Rule {
        meta: RuleMeta {
            id: "cognitive-complexity",
            description: "Function is too cognitively complex",
            title: "Simplify complex functions",
            why: "Deeply nested logic is hard to follow and test; split this function into smaller pieces.",
            severity: Severity::Info,
        },
        scope: RuleScope::File,
        requires_scope: false,
        run: Some(rules::cognitive_complexity::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-duplicate-string",
            description: "String literal duplicated across the file",
            title: "Extract duplicated strings",
            why: "The same text repeated many times drifts when edited; extract it into one named constant.",
            severity: Severity::Info,
        },
        scope: RuleScope::File,
        requires_scope: false,
        run: Some(rules::no_duplicate_string::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-identical-conditions",
            description: "Branch condition duplicates an earlier one in the same chain",
            title: "Remove duplicate conditions",
            why: "A repeated condition in the same chain can never run; remove the branch or change it.",
            severity: Severity::Warning,
        },
        scope: RuleScope::File,
        requires_scope: false,
        run: Some(rules::no_identical_conditions::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-redundant-boolean",
            description: "Redundant boolean comparison or double negation",
            title: "Remove redundant booleans",
            why: "Comparing with true or false, or writing '!!', adds noise; use the value directly.",
            severity: Severity::Info,
        },
        scope: RuleScope::File,
        requires_scope: false,
        run: Some(rules::no_redundant_boolean::run),
    },
    Rule {
        meta: RuleMeta {
            id: "prefer-immediate-return",
            description: "Variable declared only to be returned immediately",
            title: "Return the expression directly",
            why: "A variable that is only returned adds a step; return the expression instead.",
            severity: Severity::Info,
        },
        scope: RuleScope::File,
        requires_scope: false,
        run: Some(rules::prefer_immediate_return::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-collapsible-if",
            description: "Nested 'if' can be merged with the outer condition",
            title: "Merge nested ifs",
            why: "An inner 'if' with no 'else' can join the outer condition with '&&', removing a nesting level.",
            severity: Severity::Info,
        },
        scope: RuleScope::File,
        requires_scope: false,
        run: Some(rules::no_collapsible_if::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-identical-expressions",
            description: "Identical expression used on both sides of a binary operator",
            title: "Fix identical operands",
            why: "Comparing or combining a value with itself is usually a copy-paste mistake; check both sides.",
            severity: Severity::Warning,
        },
        scope: RuleScope::File,
        requires_scope: false,
        run: Some(rules::no_identical_expressions::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-duplicated-branches",
            description: "Two branches share the same implementation",
            title: "Merge duplicated branches",
            why: "Two branches doing the same thing make the condition pointless; merge them or change one.",
            severity: Severity::Warning,
        },
        scope: RuleScope::File,
        requires_scope: false,
        run: Some(rules::no_duplicated_branches::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-all-duplicated-branches",
            description: "All branches share the same implementation",
            title: "Remove identical branches",
            why: "When every branch does the same thing the condition is useless; keep one branch only.",
            severity: Severity::Warning,
        },
        scope: RuleScope::File,
        requires_scope: false,
        run: Some(rules::no_all_duplicated_branches::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-useless-catch",
            description: "Catch block only rethrows the caught error",
            title: "Remove useless try/catch",
            why: "A catch that only rethrows adds noise and hides the stack trace; remove the try/catch.",
            severity: Severity::Info,
        },
        scope: RuleScope::File,
        requires_scope: false,
        run: Some(rules::no_useless_catch::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-redundant-jump",
            description: "Redundant 'return' or 'continue' at the end of a block",
            title: "Remove redundant jumps",
            why: "A 'return' or 'continue' at the end of a block does nothing; delete it.",
            severity: Severity::Info,
        },
        scope: RuleScope::File,
        requires_scope: false,
        run: Some(rules::no_redundant_jump::run),
    },
    Rule {
        meta: RuleMeta {
            id: "prefer-single-boolean-return",
            description: "Boolean return wrapped in an 'if'/'else'",
            title: "Return the condition",
            why: "Returning true or false from an if/else is just the condition; return it directly.",
            severity: Severity::Info,
        },
        scope: RuleScope::File,
        requires_scope: false,
        run: Some(rules::prefer_single_boolean_return::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-nested-switch",
            description: "'switch' statement nested inside another 'switch'",
            title: "Flatten nested switches",
            why: "A switch inside a switch is hard to follow; move the inner one into its own function.",
            severity: Severity::Info,
        },
        scope: RuleScope::File,
        requires_scope: false,
        run: Some(rules::no_nested_switch::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-nested-template-literals",
            description: "Template literal nested inside another template literal",
            title: "Flatten nested templates",
            why: "A template inside a template is hard to read; build the inner text in its own variable.",
            severity: Severity::Info,
        },
        scope: RuleScope::File,
        requires_scope: false,
        run: Some(rules::no_nested_template_literals::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-small-switch",
            description: "'switch' has too few 'case' clauses",
            title: "Replace small switches",
            why: "A switch with one or two cases reads better as an if/else; rewrite it.",
            severity: Severity::Info,
        },
        scope: RuleScope::File,
        requires_scope: false,
        run: Some(rules::no_small_switch::run),
    },
    Rule {
        meta: RuleMeta {
            id: "max-switch-cases",
            description: "'switch' has too many 'case' clauses",
            title: "Reduce switch cases",
            why: "A very large switch is really a lookup table; move the mapping into data.",
            severity: Severity::Info,
        },
        scope: RuleScope::File,
        requires_scope: false,
        run: Some(rules::max_switch_cases::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-same-line-conditional",
            description: "Conditional starts on the same line as the previous statement",
            title: "Put conditionals on new lines",
            why: "An 'if' on the same line as the previous statement is easy to miss; move it down.",
            severity: Severity::Info,
        },
        scope: RuleScope::File,
        requires_scope: false,
        run: Some(rules::no_same_line_conditional::run),
    },
    Rule {
        meta: RuleMeta {
            id: "non-existent-operator",
            description: "Non-existent operator such as '=+' or '=-'",
            title: "Fix the mistyped operator",
            why: "'=+' and '=-' are not real operator combinations; write '+=', '-=' or '!=' instead.",
            severity: Severity::Warning,
        },
        scope: RuleScope::File,
        requires_scope: false,
        run: Some(rules::non_existent_operator::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-self-assign",
            description: "Variable assigned to itself",
            title: "Remove self-assignment",
            why: "Assigning a variable to itself changes nothing and usually hides a typo; remove or fix it.",
            severity: Severity::Warning,
        },
        scope: RuleScope::File,
        requires_scope: false,
        run: Some(rules::no_self_assign::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-constant-condition",
            description: "Constant condition in 'if' or ternary expression",
            title: "Remove constant conditions",
            why: "A condition that never changes makes the branch predictable; remove it or use the intended value.",
            severity: Severity::Warning,
        },
        scope: RuleScope::File,
        requires_scope: false,
        run: Some(rules::no_constant_condition::run),
    },
    Rule {
        meta: RuleMeta {
            id: "use-isnan",
            description: "Comparison with 'NaN' instead of Number.isNaN",
            title: "Use Number.isNaN",
            why: "Any comparison with NaN is always false; use Number.isNaN(...) to test for it.",
            severity: Severity::Warning,
        },
        scope: RuleScope::File,
        requires_scope: false,
        run: Some(rules::use_isnan::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-array-constructor",
            description: "Array constructed with the 'Array' constructor",
            title: "Use array literals",
            why: "The 'Array' constructor has surprising edge cases; write an array literal with brackets instead.",
            severity: Severity::Info,
        },
        scope: RuleScope::File,
        requires_scope: false,
        run: Some(rules::no_array_constructor::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-new-wrappers",
            description: "Primitive wrapper object created with 'new'",
            title: "Avoid wrapper objects",
            why: "'new String', 'new Number' and 'new Boolean' create objects, not primitives; call them without 'new'.",
            severity: Severity::Warning,
        },
        scope: RuleScope::File,
        requires_scope: false,
        run: Some(rules::no_new_wrappers::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-duplicate-imports",
            description: "Module imported more than once",
            title: "Merge duplicate imports",
            why: "Importing the same module twice splits its bindings; combine them into one import.",
            severity: Severity::Info,
        },
        scope: RuleScope::File,
        requires_scope: false,
        run: Some(rules::no_duplicate_imports::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-unused-vars",
            description: "Variable or import is declared but never used",
            title: "Remove unused variables",
            why: "An unused declaration is dead code that confuses readers; remove it or use it.",
            severity: Severity::Info,
        },
        scope: RuleScope::File,
        requires_scope: true,
        run: Some(rules::no_unused_vars::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-shadow",
            description: "Variable shadows a binding in an outer scope",
            title: "Avoid shadowed names",
            why: "A name reused from an outer scope hides the original and invites bugs; rename this binding.",
            severity: Severity::Info,
        },
        scope: RuleScope::File,
        requires_scope: true,
        run: Some(rules::no_shadow::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-use-before-define",
            description: "Binding is used before its declaration",
            title: "Define before use",
            why: "Using a value before its declaration can fail at runtime; move the declaration earlier.",
            severity: Severity::Warning,
        },
        scope: RuleScope::File,
        requires_scope: true,
        run: Some(rules::no_use_before_define::run),
    },
    Rule {
        meta: RuleMeta {
            id: "prefer-const",
            description: "'let' variable is never reassigned; prefer 'const'",
            title: "Prefer const",
            why: "'let' invites reassignment; if the value never changes, declare it with 'const'.",
            severity: Severity::Info,
        },
        scope: RuleScope::File,
        requires_scope: true,
        run: Some(rules::prefer_const::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-redeclare",
            description: "Binding is declared more than once in the same scope",
            title: "Remove duplicate declarations",
            why: "Declaring the same name twice in one scope is confusing and often a mistake; keep one.",
            severity: Severity::Info,
        },
        scope: RuleScope::File,
        requires_scope: true,
        run: Some(rules::no_redeclare::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-unreachable",
            description: "Statement is unreachable after a statement that transfers control",
            title: "Remove unreachable code",
            why: "Code after a return, throw or break never runs; delete it so readers are not misled.",
            severity: Severity::Warning,
        },
        scope: RuleScope::File,
        requires_scope: false,
        run: Some(rules::no_unreachable::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-fallthrough",
            description: "Switch case falls through to the next case",
            title: "Stop switch fallthrough",
            why: "A case that falls into the next one is easy to miss; add 'break' or comment the intent.",
            severity: Severity::Warning,
        },
        scope: RuleScope::File,
        requires_scope: false,
        run: Some(rules::no_fallthrough::run),
    },
    Rule {
        meta: RuleMeta {
            id: "consistent-return",
            description: "Function returns a value on some paths but not on others",
            title: "Return consistently",
            why: "Mixing value and empty returns gives callers an unexpected undefined; return a value on every path.",
            severity: Severity::Warning,
        },
        scope: RuleScope::File,
        requires_scope: false,
        run: Some(rules::consistent_return::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-one-iteration-loop",
            description: "Loop body always exits on its first iteration",
            title: "Remove single-pass loops",
            why: "A loop that always exits on its first pass is really a block; replace it with its body.",
            severity: Severity::Info,
        },
        scope: RuleScope::File,
        requires_scope: false,
        run: Some(rules::no_one_iteration_loop::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-unused-collection",
            description: "Collection is only mutated; its contents are never read",
            title: "Remove unused collections",
            why: "A collection that is only filled is dead weight; remove it or read its contents.",
            severity: Severity::Info,
        },
        scope: RuleScope::File,
        requires_scope: true,
        run: Some(rules::no_unused_collection::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-element-overwrite",
            description: "Assignment overwrites an unread adjacent assignment to the same target",
            title: "Remove overwritten assignments",
            why: "The first assignment is replaced before anyone reads it; delete the useless write.",
            severity: Severity::Warning,
        },
        scope: RuleScope::File,
        requires_scope: false,
        run: Some(rules::no_element_overwrite::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-ignored-return",
            description: "Return value of a pure function call is ignored",
            title: "Use the returned value",
            why: "Ignoring the result of a pure call wastes the work; use the value or drop the call.",
            severity: Severity::Warning,
        },
        scope: RuleScope::File,
        requires_scope: false,
        run: Some(rules::no_ignored_return::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-gratuitous-expressions",
            description: "Condition contains a self-contradicting or redundant expression",
            title: "Simplify constant conditions",
            why: "An expression that is always true or false makes the condition meaningless; simplify it.",
            severity: Severity::Warning,
        },
        scope: RuleScope::File,
        requires_scope: false,
        run: Some(rules::no_gratuitous_expressions::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-eval",
            description: "Direct call to 'eval' executes arbitrary code",
            title: "Remove eval calls",
            why: "'eval' runs arbitrary strings with the app's permissions; refactor the code to avoid it.",
            severity: Severity::Error,
        },
        scope: RuleScope::File,
        requires_scope: false,
        run: Some(rules::no_eval::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-implied-eval",
            description: "Timer or execScript called with a string evaluated as code",
            title: "Pass functions to timers",
            why: "A string passed to a timer is evaluated as code; pass a function instead.",
            severity: Severity::Error,
        },
        scope: RuleScope::File,
        requires_scope: false,
        run: Some(rules::no_implied_eval::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-new-func",
            description: "The 'Function' constructor compiles a string into code",
            title: "Avoid the Function constructor",
            why: "The 'Function' constructor runs strings like eval and blocks optimizations; use a real function.",
            severity: Severity::Error,
        },
        scope: RuleScope::File,
        requires_scope: false,
        run: Some(rules::no_new_func::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-script-url",
            description: "String literal uses a 'javascript:' URL",
            title: "Avoid javascript: URLs",
            why: "'javascript:' links run code when clicked and bypass CSP; use a normal URL or an event handler.",
            severity: Severity::Error,
        },
        scope: RuleScope::File,
        requires_scope: false,
        run: Some(rules::no_script_url::run),
    },
    Rule {
        meta: RuleMeta {
            id: "hardcoded-secrets",
            description: "Possible hard-coded secret in a string assignment",
            title: "Remove hard-coded secrets",
            why: "Secrets in source end up in version control; load them from configuration or a vault.",
            severity: Severity::Error,
        },
        scope: RuleScope::File,
        requires_scope: false,
        run: Some(rules::hardcoded_secrets::run),
    },
    // Cross-file rule: orchestrated by the CLI because it needs every source at once,
    // but still registered here so SARIF and config validation have one catalog.
    Rule {
        meta: RuleMeta {
            id: "duplicate-code",
            description: "Copy-pasted block of code",
            title: "Extract duplicated code",
            why: "Copy-pasted blocks drift apart when one copy changes; extract the shared logic into one place.",
            severity: Severity::Warning,
        },
        scope: RuleScope::Project,
        requires_scope: false,
        run: None,
    },
    Rule {
        meta: RuleMeta {
            id: "no-alphabetical-sort",
            description: "Array sorted without a compare function",
            title: "Provide a sort comparator",
            why: "Without a comparator, sort() compares elements as strings, so numbers and accented text can come out in the wrong order.",
            severity: Severity::Warning,
        },
        scope: RuleScope::File,
        requires_scope: false,
        run: Some(rules::no_alphabetical_sort::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-nested-ternary",
            description: "Ternary expression nested inside another ternary expression",
            title: "Avoid nested ternaries",
            why: "Chained ternaries are hard to read; use if/else or a helper instead.",
            severity: Severity::Info,
        },
        scope: RuleScope::File,
        requires_scope: false,
        run: Some(rules::no_nested_ternary::run),
    },
    Rule {
        meta: RuleMeta {
            id: "no-array-index-key",
            description: "JSX key uses the index parameter of a map callback",
            title: "Use stable keys",
            why: "Index keys change when the list is reordered, so React can reuse the wrong DOM node.",
            severity: Severity::Warning,
        },
        scope: RuleScope::File,
        requires_scope: true,
        run: Some(rules::no_array_index_key::run),
    },
];

/// Severity of a registered rule id.
///
/// Only ids produced by [`RULES`] are expected: the diagnostic builders derive the level
/// from the same registry entry, so an unknown id means a caller passed a typo. It still
/// falls back to [`Severity::Info`] rather than panicking, because losing a diagnostic
/// over a metadata lookup would be worse than under-reporting its level.
pub fn severity_of(rule: &str) -> Severity {
    RULES
        .iter()
        .find(|registered| registered.meta.id == rule)
        .map_or(Severity::Info, |registered| registered.meta.severity)
}

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
            assert!(
                !rule.meta.title.trim().is_empty(),
                "rule '{}' has an empty title",
                rule.meta.id
            );
            assert!(
                !rule.meta.why.trim().is_empty(),
                "rule '{}' has an empty why",
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
            "no-unused-vars",
            "no-shadow",
            "no-use-before-define",
            "prefer-const",
            "no-redeclare",
            "no-unreachable",
            "no-fallthrough",
            "consistent-return",
            "no-one-iteration-loop",
            "no-unused-collection",
            "no-element-overwrite",
            "no-ignored-return",
            "no-gratuitous-expressions",
            "no-eval",
            "no-implied-eval",
            "no-new-func",
            "no-script-url",
            "hardcoded-secrets",
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
                "no-unused-vars",
                "no-shadow",
                "no-use-before-define",
                "prefer-const",
                "no-redeclare",
                "no-unreachable",
                "no-fallthrough",
                "consistent-return",
                "no-one-iteration-loop",
                "no-unused-collection",
                "no-element-overwrite",
                "no-ignored-return",
                "no-gratuitous-expressions",
                "no-eval",
                "no-implied-eval",
                "no-new-func",
                "no-script-url",
                "hardcoded-secrets",
                "no-alphabetical-sort",
                "no-nested-ternary",
                "no-array-index-key",
            ]
        );
    }

    #[test]
    fn every_rule_has_a_matching_severity_lookup() {
        for rule in RULES {
            assert_eq!(severity_of(rule.meta.id), rule.meta.severity);
        }
        assert_eq!(severity_of("not-a-registered-rule"), Severity::Info);
    }

    #[test]
    fn scope_rules_require_the_scope_graph() {
        for id in [
            "no-unused-vars",
            "no-shadow",
            "no-use-before-define",
            "prefer-const",
            "no-redeclare",
            "no-unused-collection",
            "no-array-index-key",
        ] {
            let rule = RULES
                .iter()
                .find(|rule| rule.meta.id == id)
                .unwrap_or_else(|| panic!("missing rule '{id}'"));
            assert!(rule.requires_scope, "'{id}' should require scope analysis");
        }

        for rule in RULES.iter().filter(|rule| {
            !matches!(
                rule.meta.id,
                "no-unused-vars"
                    | "no-shadow"
                    | "no-use-before-define"
                    | "prefer-const"
                    | "no-redeclare"
                    | "no-unused-collection"
                    | "no-array-index-key"
            )
        }) {
            assert!(
                !rule.requires_scope,
                "existing rule '{}' should not opt into scope analysis",
                rule.meta.id
            );
        }
    }
}
