//! TypeScript/TSX rules. Both dialects share one rule set: the TSX grammar is a
//! superset (adds JSX), and every rule compiles its queries against whichever grammar
//! produced the tree, via `tree.language()`.
//!
//! Implementations live one-per-file under [`rules`]; registration and metadata live in
//! `crate::rule::RULES`. Tests stay here because they exercise the full analyzer and
//! assert cross-rule diagnostic ordering.

pub mod rules;

#[cfg(test)]
mod tests {
    use crate::{Config, Diagnostic, Language};

    fn analyze(source: &str, config: &Config) -> Vec<Diagnostic> {
        crate::analyze(source, Language::TypeScript, config)
    }

    fn rules(source: &str) -> Vec<&'static str> {
        analyze(source, &Config::default()).iter().map(|d| d.rule).collect()
    }

    #[test]
    fn flags_console_log() {
        assert_eq!(rules("console.log('hi');"), vec!["no-console"]);
    }

    #[test]
    fn flags_debugger() {
        assert_eq!(rules("debugger;"), vec!["no-debugger"]);
    }

    #[test]
    fn flags_var() {
        assert_eq!(rules("var x = 1;"), vec!["no-var"]);
    }

    #[test]
    fn allows_let_and_const() {
        assert!(rules("let x = 1; const y = 2;").is_empty());
    }

    #[test]
    fn flags_loose_equality() {
        assert_eq!(rules("if (a == b) {}"), vec!["eqeqeq"]);
        assert_eq!(rules("if (a != b) {}"), vec!["eqeqeq"]);
    }

    #[test]
    fn allows_strict_equality() {
        assert!(rules("if (a === b) {}").is_empty());
    }

    #[test]
    fn flags_explicit_any() {
        assert_eq!(rules("function f(x: any) {}"), vec!["no-explicit-any"]);
    }

    #[test]
    fn allows_concrete_types() {
        assert!(rules("function f(x: number) {}").is_empty());
    }

    #[test]
    fn flags_empty_catch() {
        assert_eq!(rules("try { f(); } catch (e) {}"), vec!["no-empty-catch"]);
    }

    #[test]
    fn allows_catch_with_body() {
        assert!(rules("try { f(); } catch (e) { log(e); }").is_empty());
    }

    #[test]
    fn clean_code_has_no_diagnostics() {
        assert!(rules("const x = 1;\nfunction f() { return x; }").is_empty());
    }

    #[test]
    fn flags_string_duplicated_three_times() {
        // Only the 3rd+ occurrence is flagged — the first two uses of a string aren't
        // yet a pattern worth extracting into a constant.
        let src = r#"
            const a = "this is a long duplicated string";
            const b = "this is a long duplicated string";
            const c = "this is a long duplicated string";
        "#;
        assert_eq!(rules(src), vec!["no-duplicate-string"]);
    }

    #[test]
    fn flags_one_diagnostic_per_occurrence_past_the_threshold() {
        let src = r#"
            const a = "this is a long duplicated string";
            const b = "this is a long duplicated string";
            const c = "this is a long duplicated string";
            const d = "this is a long duplicated string";
        "#;
        assert_eq!(rules(src), vec!["no-duplicate-string", "no-duplicate-string"]);
    }

    #[test]
    fn allows_string_duplicated_twice() {
        let src = r#"
            const a = "this is a long duplicated string";
            const b = "this is a long duplicated string";
        "#;
        assert!(rules(src).is_empty());
    }

    #[test]
    fn allows_short_duplicated_strings() {
        let src = r#"const a = "id"; const b = "id"; const c = "id";"#;
        assert!(rules(src).is_empty());
    }

    #[test]
    fn flags_identical_else_if_condition() {
        let src = "
            function f(x: number) {
                if (x === 1) { a(); } else if (x === 1) { b(); }
            }
        ";
        assert_eq!(rules(src), vec!["no-identical-conditions"]);
    }

    #[test]
    fn allows_distinct_else_if_conditions() {
        let src = "
            function f(x: number) {
                if (x === 1) { a(); } else if (x === 2) { b(); } else { c(); }
            }
        ";
        assert!(rules(src).is_empty());
    }

    #[test]
    fn allows_identical_conditions_in_separate_if_chains() {
        let src = "
            function f(x: number) {
                if (x === 1) { a(); }
                if (x === 1) { b(); }
            }
        ";
        assert!(rules(src).is_empty());
    }

    #[test]
    fn flags_comparison_to_boolean_literal() {
        assert_eq!(rules("if (a === true) {}"), vec!["no-redundant-boolean"]);
        // `==` also triggers eqeqeq independently — both rules are legitimately correct here.
        assert_eq!(rules("if (false == a) {}"), vec!["eqeqeq", "no-redundant-boolean"]);
    }

    #[test]
    fn flags_double_negation() {
        assert_eq!(rules("const x = !!a;"), vec!["no-redundant-boolean"]);
    }

    #[test]
    fn allows_plain_boolean_expressions() {
        assert!(rules("if (a) {} if (!a) {}").is_empty());
    }

    #[test]
    fn flags_declare_then_immediately_return() {
        let src = "function f() { const result = compute(); return result; }";
        assert_eq!(rules(src), vec!["prefer-immediate-return"]);
    }

    #[test]
    fn allows_return_of_a_different_identifier() {
        let src = "function f() { const result = compute(); return other; }";
        assert!(rules(src).is_empty());
    }

    #[test]
    fn allows_declaration_used_before_return() {
        let src = "function f() { const result = compute(); log(result); return result; }";
        assert!(rules(src).is_empty());
    }

    #[test]
    fn flags_collapsible_nested_if() {
        let src = "function f(a: boolean, b: boolean) { if (a) { if (b) { g(); } } }";
        assert_eq!(rules(src), vec!["no-collapsible-if"]);
    }

    #[test]
    fn allows_nested_if_with_else() {
        let src = "function f(a: boolean, b: boolean) { if (a) { if (b) { g(); } else { h(); } } }";
        assert!(rules(src).is_empty());
    }

    #[test]
    fn allows_nested_if_when_outer_has_more_statements() {
        let src = "function f(a: boolean, b: boolean) { if (a) { setup(); if (b) { g(); } } }";
        assert!(rules(src).is_empty());
    }

    #[test]
    fn config_can_disable_a_rule() {
        let src = "console.log('hi');";
        assert_eq!(rules(src), vec!["no-console"]);

        let mut config = Config::default();
        config.disabled_rules.insert("no-console".to_string());
        assert!(analyze(src, &config).is_empty());
    }

    #[test]
    fn config_can_raise_the_cognitive_complexity_threshold() {
        let src = "
            function f(a: boolean, b: boolean, c: boolean, d: boolean, e: boolean, g: boolean) {
                if (a) { if (b) { if (c) { if (d) { if (e) { if (g) { return 1; } } } } } }
            }
        ";
        // Each nesting level except the innermost is independently collapsible, hence 5
        // no-collapsible-if hits; what this test actually cares about is the presence
        // (then absence) of cognitive-complexity once the threshold is raised.
        assert!(rules(src).contains(&"cognitive-complexity"));

        let mut config = Config::default();
        config.cognitive_complexity_threshold = 25;
        let diagnostics = analyze(src, &config);
        assert!(!diagnostics.iter().any(|d| d.rule == "cognitive-complexity"));
    }
}
