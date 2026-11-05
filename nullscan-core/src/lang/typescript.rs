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
        // Reassign `x` so this test keeps testing syntax choice rather than triggering
        // the separate `prefer-const` rule.
        assert!(rules("let x = 1; x = 2; const y = 2;").is_empty());
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
        // Keep the two `if`s on separate lines: same-line conditionals are flagged by
        // `no-same-line-conditional` independently of this test's intent.
        assert!(rules("if (a) {}\nif (!a) {}").is_empty());
    }

    #[test]
    fn flags_declare_then_immediately_return() {
        let src = "function f() { const result = compute(); return result; }";
        assert_eq!(rules(src), vec!["prefer-immediate-return"]);
    }

    #[test]
    fn allows_return_of_a_different_identifier() {
        // Keep `result` used so `no-unused-vars` doesn't fire; the point here is that
        // returning a different identifier is not `prefer-immediate-return`.
        let src = "function f() { const result = compute(); void result; return other; }";
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
        let src = "function f(a: boolean, b: boolean) { if (a) { setup();\nif (b) { g(); } } }";
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

    #[test]
    fn flags_identical_binary_operands() {
        assert_eq!(
            rules("const same = a === a;"),
            vec!["no-identical-expressions"]
        );
        assert_eq!(
            rules("const overlap = a < a;"),
            vec!["no-identical-expressions"]
        );
    }

    #[test]
    fn allows_identical_operands_for_plus_and_multiply() {
        assert!(rules("const sum = x + x;\nconst square = x * x;").is_empty());
    }

    #[test]
    fn flags_duplicated_non_final_branch() {
        let src = "function f(x: number) { if (x === 1) { a(); } else if (x === 2) { b(); } else { a(); } }";
        assert_eq!(rules(src), vec!["no-duplicated-branches"]);
    }

    #[test]
    fn allows_distinct_branches() {
        let src = "function f(x: number) { if (x === 1) { a(); } else if (x === 2) { b(); } else { c(); } }";
        assert!(rules(src).is_empty());
    }

    #[test]
    fn flags_all_duplicated_branches_once() {
        assert_eq!(
            rules("function f(x: number) { if (x === 1) { same(); } else { same(); } }"),
            vec!["no-all-duplicated-branches"]
        );
        assert_eq!(
            rules("const value = x ? same() : same();"),
            vec!["no-all-duplicated-branches"]
        );
    }

    #[test]
    fn flags_useless_catch() {
        assert_eq!(
            rules("try { f(); } catch (e) { throw e; }"),
            vec!["no-useless-catch"]
        );
        assert_eq!(
            rules("try { f(); } catch (e) { /* rethrow */ throw e; }"),
            vec!["no-useless-catch"]
        );
    }

    #[test]
    fn allows_catch_that_does_more_than_rethrow() {
        assert!(rules("try { f(); } catch (e) { log(e); throw e; }").is_empty());
    }

    #[test]
    fn flags_redundant_return_and_continue() {
        assert_eq!(
            rules("function f() { return; }"),
            vec!["no-redundant-jump"]
        );
        assert_eq!(
            rules("function f() { for (;;) { continue; } }"),
            vec!["no-redundant-jump"]
        );
        assert_eq!(
            rules("function f() { return; /* done */ }"),
            vec!["no-redundant-jump"]
        );
    }

    #[test]
    fn allows_valued_return_and_break() {
        assert!(
            rules("function f() { return 1; }\nfunction g() { for (;;) { break; } }").is_empty()
        );
    }

    #[test]
    fn flags_boolean_return_wrapped_in_if_else() {
        let src = "function f(flag: boolean): boolean { if (flag) { return true; } else { return false; } }";
        assert_eq!(rules(src), vec!["prefer-single-boolean-return"]);
    }

    #[test]
    fn allows_guard_style_boolean_returns() {
        let src = "function f(flag: boolean): boolean { if (flag) { return true; }\nreturn false; }";
        assert!(rules(src).is_empty());
    }

    #[test]
    fn flags_nested_switch_without_a_function_boundary() {
        let src = "
            function f(x: number) {
                switch (x) {
                    case 1: break;
                    case 2: break;
                    case 3:
                        switch (x) {
                            case 1: break;
                            case 2: break;
                            case 3: break;
                            default: break;
                        }
                    default: break;
                }
            }
        ";
        assert_eq!(rules(src), vec!["no-nested-switch"]);
    }

    #[test]
    fn allows_separate_switches() {
        let src = "
            function f(x: number) {
                switch (x) { case 1: break; case 2: break; case 3: break; }
                switch (x) { case 1: break; case 2: break; case 3: break; }
            }
        ";
        assert!(rules(src).is_empty());
    }

    #[test]
    fn flags_nested_template_literals() {
        assert_eq!(
            rules("const message = `outer ${`inner`}`;"),
            vec!["no-nested-template-literals"]
        );
        assert!(rules("const message = `outer ${name}`;").is_empty());
    }

    #[test]
    fn tsx_detects_nested_template_literals() {
        let src = "export function Label({ name }: { name: string }) { const text = `outer ${`inner ${name}`}`; return <span>{text}</span>; }";
        let diagnostics = crate::analyze(src, Language::Tsx, &Config::default());
        let rules: Vec<_> = diagnostics.iter().map(|d| d.rule).collect();
        assert_eq!(rules, vec!["no-nested-template-literals"]);
    }

    #[test]
    fn flags_small_switch() {
        let src = "function f(x: number) { switch (x) { case 1: break; case 2: break; } }";
        assert_eq!(rules(src), vec!["no-small-switch"]);
        let three = "function f(x: number) { switch (x) { case 1: break; case 2: break; case 3: break; } }";
        assert!(rules(three).is_empty());
    }

    #[test]
    fn flags_switch_with_too_many_cases() {
        assert_eq!(rules(&switch_source(11)), vec!["max-switch-cases"]);
        assert!(rules(&switch_source(10)).is_empty());
    }

    #[test]
    fn config_controls_switch_case_thresholds() {
        let defaults = Config::default();
        assert_eq!(defaults.max_switch_cases, 10);
        assert_eq!(defaults.no_small_switch_min_cases, 3);

        let small = "function f(x: number) { switch (x) { case 1: break; case 2: break; } }";
        assert_eq!(rules(small), vec!["no-small-switch"]);
        let mut config = Config::default();
        config.no_small_switch_min_cases = 2;
        assert!(analyze(small, &config).is_empty());

        let large = switch_source(11);
        assert_eq!(rules(&large), vec!["max-switch-cases"]);
        let mut config = Config::default();
        config.max_switch_cases = 20;
        assert!(analyze(&large, &config).is_empty());
    }

    #[test]
    fn flags_same_line_conditional() {
        assert_eq!(
            rules("const x = 1; if (x) {}"),
            vec!["no-same-line-conditional"]
        );
        assert!(rules("const x = 1;\nif (x) {}").is_empty());
        assert!(rules("if (a) {}\nelse if (b) {}").is_empty());
    }

    #[test]
    fn flags_non_existent_operators() {
        assert_eq!(
            rules("let a = 1;\na=+b;"),
            vec!["non-existent-operator"]
        );
        assert_eq!(
            rules("let a = 1;\na=-b;"),
            vec!["non-existent-operator"]
        );
        assert_eq!(
            rules("let a = 1;\na=!b;"),
            vec!["non-existent-operator"]
        );
        assert!(rules("let a = 1;\na = +b;\na = -b;\na = !b;").is_empty());
    }

    #[test]
    fn flags_self_assignment() {
        assert_eq!(rules("x = x;"), vec!["no-self-assign"]);
        assert!(rules("x = y;").is_empty());
    }

    #[test]
    fn flags_constant_conditions() {
        assert_eq!(rules("if (true) {}"), vec!["no-constant-condition"]);
        assert_eq!(rules("if (1 + 2) {}"), vec!["no-constant-condition"]);
        assert_eq!(
            rules("const value = false ? 1 : 2;"),
            vec!["no-constant-condition"]
        );
        assert_eq!(rules("if (!true) {}"), vec!["no-constant-condition"]);
        assert!(rules("if (value) {}\nwhile (true) {}").is_empty());
    }

    #[test]
    fn flags_nan_comparisons() {
        assert_eq!(
            rules("const bad = value === NaN;"),
            vec!["use-isnan"]
        );
        assert_eq!(
            rules("const bad = value != Number.NaN;"),
            vec!["eqeqeq", "use-isnan"]
        );
        assert!(rules("const bad = value === other;").is_empty());
    }

    #[test]
    fn flags_array_constructor_edge_cases() {
        assert_eq!(
            rules("const a = new Array();"),
            vec!["no-array-constructor"]
        );
        assert_eq!(
            rules("const b = Array(1, 2);"),
            vec!["no-array-constructor"]
        );
        assert_eq!(
            rules("const c = new Array(\"x\");"),
            vec!["no-array-constructor"]
        );
        assert!(rules("const d = new Array(500);\nconst e = Array.from(items);").is_empty());
    }

    #[test]
    fn flags_new_primitive_wrappers() {
        assert_eq!(
            rules("const s = new String(\"x\");"),
            vec!["no-new-wrappers"]
        );
        assert_eq!(
            rules("const n = new Number(1);"),
            vec!["no-new-wrappers"]
        );
        assert_eq!(
            rules("const b = new Boolean(false);"),
            vec!["no-new-wrappers"]
        );
        assert!(
            rules("const s = String(\"x\");\nconst n = Number(1);\nconst b = Boolean(false);")
                .is_empty()
        );
    }

    #[test]
    fn flags_duplicate_imports() {
        // Use the imported names so the separate `no-unused-vars` rule stays quiet.
        let duplicated = "import { a } from \"./a\";\nimport { b } from \"./a\";\nuse(a, b);";
        assert_eq!(rules(duplicated), vec!["no-duplicate-imports"]);
        let distinct = "import { a } from \"./a\";\nimport { b } from \"./b\";\nuse(a, b);";
        assert!(rules(distinct).is_empty());
    }

    #[test]
    fn flags_unused_imports_and_local_variables() {
        assert_eq!(
            rules("import { unused } from \"./m\";"),
            vec!["no-unused-vars"]
        );
        assert_eq!(
            rules("function f() { const unused = 1; }"),
            vec!["no-unused-vars"]
        );
        assert!(rules("function f() { const used = 1; log(used); }").is_empty());
        // Module-level bindings are skipped, and export specifier names are not reads.
        assert!(rules("const exported = 1; export { exported };").is_empty());
        // Parameters and function declarations are out of scope for this rule.
        assert!(rules("function f(unusedParam) { return 1; }").is_empty());
    }

    #[test]
    fn flags_each_unused_destructured_binding_separately() {
        let diagnostics = analyze(
            "function f() { const { used, unused } = source; return used; }",
            &Config::default(),
        );
        let rule_ids: Vec<_> = diagnostics.iter().map(|d| d.rule).collect();
        assert_eq!(rule_ids, vec!["no-unused-vars"]);
        assert_eq!(diagnostics[0].message, "'unused' is declared but never used");
    }

    #[test]
    fn unused_vars_ignores_underscore_names_and_counts_type_uses() {
        assert!(rules("function f() { const _ignored = 1; }").is_empty());
        assert!(
            rules("import { Foo } from \"./m\";\nexport const value: Foo = make();").is_empty()
        );
        assert!(rules("function f() { const value: Foo = make(); log(value); }").is_empty());
    }

    #[test]
    fn tsx_counts_component_and_prop_references() {
        let source = "import { Button } from \"./b\";\nexport function Panel({ label }: { label: string }) { const text = label; return <Button title={text} />; }";
        let diagnostics = crate::analyze(source, Language::Tsx, &Config::default());
        let rule_ids: Vec<_> = diagnostics.iter().map(|d| d.rule).collect();
        assert!(rule_ids.is_empty(), "unexpected diagnostics: {rule_ids:?}");
    }

    #[test]
    fn flags_shadowed_binding() {
        let src = "function f() { const value = 1; { const value = 2; use(value); } return value; }";
        assert_eq!(rules(src), vec!["no-shadow"]);
        assert!(
            rules("function f() { const value = 1; { const other = 2; use(other); } return value; }")
                .is_empty()
        );
    }

    #[test]
    fn flags_use_before_define_in_the_same_scope() {
        let src = "function f() { use(value); let value = 1; value = 2; return value; }";
        assert_eq!(rules(src), vec!["no-use-before-define"]);
        // `var` is hoisted (the `no-var` rule may fire independently).
        let var_source = "function f() { use(value); var value = 1; log(value); }";
        assert!(!rules(var_source).contains(&"no-use-before-define"));
        // A nested function reference is deliberately not reported.
        assert!(
            rules(
                "function f() { function g() { use(value); } let value = 1; value = 2; }"
            )
            .is_empty()
        );
    }

    #[test]
    fn flags_let_that_is_never_reassigned() {
        assert_eq!(
            rules("function f() { let value = 1; log(value); }"),
            vec!["prefer-const"]
        );
        assert_eq!(
            rules("function f() { let { a, b } = source; return a + b; }"),
            vec!["prefer-const"]
        );
        assert!(
            rules("function f() { let value = 1; value = 2; return value; }").is_empty()
        );
        assert!(rules("function f() { let value; value = 1; return value; }").is_empty());
    }

    #[test]
    fn flags_redeclaration_in_the_same_scope() {
        assert_eq!(rules("class A {}\nclass A {}"), vec!["no-redeclare"]);
        assert!(rules("class A {}\nclass B {}").is_empty());
    }

    #[test]
    fn config_can_disable_scope_rules() {
        let cases = [
            ("no-unused-vars", "function f() { const unused = 1; }"),
            (
                "no-shadow",
                "function f() { const value = 1; { const value = 2; use(value); } return value; }",
            ),
            (
                "no-use-before-define",
                "function f() { use(value); let value = 1; value = 2; return value; }",
            ),
            ("prefer-const", "function f() { let value = 1; return value; }"),
            ("no-redeclare", "class A {}\nclass A {}"),
        ];

        for (rule, source) in cases {
            assert!(
                rules(source).contains(&rule),
                "'{rule}' should fire before being disabled"
            );
            let mut config = Config::default();
            config.disabled_rules.insert(rule.to_string());
            let diagnostics = analyze(source, &config);
            assert!(
                !diagnostics.iter().any(|diagnostic| diagnostic.rule == rule),
                "'{rule}' still fired after being disabled"
            );
        }
    }

    fn switch_source(cases: usize) -> String {
        let mut source = String::from("function f(x: number): number { switch (x) {");
        for case in 1..=cases {
            source.push_str(&format!(" case {case}: return {case};"));
        }
        source.push_str(" default: return -1; } }");
        source
    }
}
