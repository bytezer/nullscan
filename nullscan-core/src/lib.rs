mod cognitive_complexity;
mod config;
mod duplication;

use std::collections::HashMap;

use tree_sitter::{Node, Parser, Query, QueryCursor, StreamingIterator, Tree};

use cognitive_complexity::cognitive_complexity_rule;

pub use config::Config;
pub use duplication::{find_duplicates, Duplicate, DuplicateLocation};

pub struct Diagnostic {
    pub rule: &'static str,
    pub line: usize,
    pub column: usize,
    pub message: String,
}

/// Parses TypeScript source and runs every rule not disabled by `config` against it.
pub fn analyze(source: &str, config: &Config) -> Vec<Diagnostic> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
        .expect("failed to load TypeScript grammar");
    let tree = parser.parse(source, None).expect("parse failed");

    let mut diagnostics = Vec::new();
    if config.rule_enabled("no-console") {
        diagnostics.extend(no_console_rule(&tree, source));
    }
    if config.rule_enabled("no-debugger") {
        diagnostics.extend(query_rule(&tree, source, "no-debugger", "unexpected 'debugger' statement", r#"
            (debugger_statement) @report
        "#));
    }
    if config.rule_enabled("no-var") {
        diagnostics.extend(query_rule(&tree, source, "no-var", "unexpected 'var', use 'let' or 'const'", r#"
            (variable_declaration "var") @report
        "#));
    }
    if config.rule_enabled("eqeqeq") {
        diagnostics.extend(query_rule(&tree, source, "eqeqeq", "expected '===' and instead saw '=='", r#"
            (binary_expression operator: "==") @report
        "#));
        diagnostics.extend(query_rule(&tree, source, "eqeqeq", "expected '!==' and instead saw '!='", r#"
            (binary_expression operator: "!=") @report
        "#));
    }
    if config.rule_enabled("no-explicit-any") {
        diagnostics.extend(no_explicit_any_rule(&tree, source));
    }
    if config.rule_enabled("no-empty-catch") {
        diagnostics.extend(no_empty_catch_rule(&tree, source));
    }
    if config.rule_enabled("cognitive-complexity") {
        diagnostics.extend(cognitive_complexity_rule(&tree, source, config.cognitive_complexity_threshold));
    }
    if config.rule_enabled("no-duplicate-string") {
        diagnostics.extend(no_duplicate_string_rule(
            &tree,
            source,
            config.duplicate_string_threshold,
            config.duplicate_string_min_length,
        ));
    }
    if config.rule_enabled("no-identical-conditions") {
        diagnostics.extend(no_identical_conditions_rule(&tree, source));
    }
    if config.rule_enabled("no-redundant-boolean") {
        diagnostics.extend(no_redundant_boolean_comparison_rule(&tree, source));
        diagnostics.extend(query_rule(&tree, source, "no-redundant-boolean", "double negation ('!!') is redundant, use Boolean(...) or the value directly", r#"
            (unary_expression operator: "!" argument: (unary_expression operator: "!")) @report
        "#));
    }
    if config.rule_enabled("prefer-immediate-return") {
        diagnostics.extend(prefer_immediate_return_rule(&tree, source));
    }
    if config.rule_enabled("no-collapsible-if") {
        diagnostics.extend(no_collapsible_if_rule(&tree, source));
    }
    diagnostics
}

/// Flags `const x = expr; return x;` — the variable adds nothing over `return expr;`.
///
/// ponytail: only the single-declarator, immediately-adjacent-statement case is
/// checked (no data-flow analysis for "declared, then only used in the return despite
/// other statements in between"). That broader case needs real liveness analysis.
fn prefer_immediate_return_rule(tree: &Tree, source: &str) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    walk_for_immediate_returns(tree.root_node(), source, &mut out);
    out
}

fn walk_for_immediate_returns(node: Node, source: &str, out: &mut Vec<Diagnostic>) {
    if matches!(node.kind(), "statement_block" | "program") {
        let mut cursor = node.walk();
        let statements: Vec<Node> = node.named_children(&mut cursor).collect();
        for pair in statements.windows(2) {
            check_immediate_return(pair[0], pair[1], source, out);
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_for_immediate_returns(child, source, out);
    }
}

fn check_immediate_return(decl: Node, ret: Node, source: &str, out: &mut Vec<Diagnostic>) {
    if decl.kind() != "lexical_declaration" || ret.kind() != "return_statement" {
        return;
    }
    if decl.named_child_count() != 1 {
        return;
    }
    let declarator = decl.named_child(0).unwrap();
    if declarator.kind() != "variable_declarator" || declarator.child_by_field_name("value").is_none() {
        return;
    }
    let Some(name_node) = declarator.child_by_field_name("name") else { return };
    if name_node.kind() != "identifier" {
        return;
    }
    let Some(returned) = ret.named_child(0) else { return };
    if returned.kind() != "identifier" {
        return;
    }
    let name = name_node.utf8_text(source.as_bytes()).unwrap_or("");
    let returned_name = returned.utf8_text(source.as_bytes()).unwrap_or("");
    if name.is_empty() || name != returned_name {
        return;
    }
    let start = decl.start_position();
    out.push(Diagnostic {
        rule: "prefer-immediate-return",
        line: start.row + 1,
        column: start.column + 1,
        message: format!("'{name}' is declared only to be returned immediately; return the expression directly"),
    });
}

/// Flags `if (a) { if (b) { ... } }` with no `else` on either `if` — it can be merged
/// into `if (a && b) { ... }`.
fn no_collapsible_if_rule(tree: &Tree, source: &str) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    walk_for_collapsible_if(tree.root_node(), source, &mut out);
    out
}

fn walk_for_collapsible_if(node: Node, source: &str, out: &mut Vec<Diagnostic>) {
    if node.kind() == "if_statement" && node.child_by_field_name("alternative").is_none() {
        if let Some(consequence) = node.child_by_field_name("consequence") {
            let inner = match consequence.kind() {
                "statement_block" if consequence.named_child_count() == 1 => consequence.named_child(0),
                "if_statement" => Some(consequence),
                _ => None,
            };
            if let Some(inner) = inner {
                if inner.kind() == "if_statement" && inner.child_by_field_name("alternative").is_none() {
                    let start = node.start_position();
                    out.push(Diagnostic {
                        rule: "no-collapsible-if",
                        line: start.row + 1,
                        column: start.column + 1,
                        message: "nested 'if' with no 'else' can be merged with the outer condition using '&&'".to_string(),
                    });
                }
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_for_collapsible_if(child, source, out);
    }
}

/// Flags string literals repeated 3+ times in the same file (mirrors SonarJS S1192);
/// short strings are exempt since they're usually incidental (e.g. `""`, `"id"`), not
/// copy-pasted business text worth extracting into a constant.
fn no_duplicate_string_rule(tree: &Tree, source: &str, threshold: usize, min_len: usize) -> Vec<Diagnostic> {
    let query = Query::new(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(), "(string) @report")
        .expect("invalid no-duplicate-string query");
    let report_idx = query.capture_index_for_name("report").unwrap();

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
    let mut occurrences: Vec<Node> = Vec::new();
    while let Some(m) = matches.next() {
        let capture = m.captures().iter().find(|c| c.index == report_idx).unwrap();
        let text = capture.node.utf8_text(source.as_bytes()).unwrap_or("");
        // Strip the surrounding quotes before measuring length.
        if text.len().saturating_sub(2) >= min_len {
            occurrences.push(capture.node);
        }
    }
    occurrences.sort_by_key(|n| n.start_byte());

    let mut counts: HashMap<&str, usize> = HashMap::new();
    for node in &occurrences {
        *counts.entry(node.utf8_text(source.as_bytes()).unwrap_or("")).or_insert(0) += 1;
    }

    let mut seen_so_far: HashMap<&str, usize> = HashMap::new();
    let mut out = Vec::new();
    for node in &occurrences {
        let text = node.utf8_text(source.as_bytes()).unwrap_or("");
        let total = counts[text];
        if total < threshold {
            continue;
        }
        let index = seen_so_far.entry(text).or_insert(0);
        *index += 1;
        if *index >= threshold {
            let start = node.start_position();
            out.push(Diagnostic {
                rule: "no-duplicate-string",
                line: start.row + 1,
                column: start.column + 1,
                message: format!("string literal {text} is duplicated {total} times, consider extracting a constant"),
            });
        }
    }
    out
}

/// Flags an `if`/`else if` whose condition text exactly repeats an earlier condition in
/// the same chain — the later branch can never run, since the earlier identical
/// condition already caught it.
///
/// ponytail: comparison is exact source text (trimmed), not semantic equivalence, so
/// `a === b` and `a===b` are treated as different conditions. Add real AST comparison
/// if that gap shows up in practice.
fn no_identical_conditions_rule(tree: &Tree, source: &str) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    walk_for_if_chains(tree.root_node(), source, &mut out);
    out
}

fn walk_for_if_chains(node: Node, source: &str, out: &mut Vec<Diagnostic>) {
    if node.kind() == "if_statement" {
        let is_chain_start = node.parent().is_none_or(|p| p.kind() != "else_clause");
        if is_chain_start {
            check_if_chain(node, source, out);
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_for_if_chains(child, source, out);
    }
}

fn check_if_chain(node: Node, source: &str, out: &mut Vec<Diagnostic>) {
    let mut seen: Vec<&str> = Vec::new();
    let mut current = Some(node);
    while let Some(n) = current {
        if let Some(condition) = n.child_by_field_name("condition") {
            let text = condition.utf8_text(source.as_bytes()).unwrap_or("").trim();
            if seen.contains(&text) {
                let start = condition.start_position();
                out.push(Diagnostic {
                    rule: "no-identical-conditions",
                    line: start.row + 1,
                    column: start.column + 1,
                    message: "this branch duplicates an earlier condition in the same if/else chain and can never run".to_string(),
                });
            } else {
                seen.push(text);
            }
        }
        current = n
            .child_by_field_name("alternative")
            .and_then(|alt| alt.named_child(0))
            .filter(|inner| inner.kind() == "if_statement");
    }
}

/// ponytail: `#eq?` text predicates aren't evaluated by `QueryCursor::matches`, so any rule
/// that needs to check a captured node's text (not just its structural shape) filters manually
/// instead of relying on the predicate — see `no_console_rule` and `no_explicit_any_rule`.
fn no_console_rule(tree: &Tree, source: &str) -> Vec<Diagnostic> {
    let query = Query::new(
        &tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        r#"
        (call_expression
          function: (member_expression
            object: (identifier) @obj)) @report
        "#,
    )
    .expect("invalid no-console query");
    let obj_idx = query.capture_index_for_name("obj").unwrap();
    let report_idx = query.capture_index_for_name("report").unwrap();

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
    let mut out = Vec::new();
    while let Some(m) = matches.next() {
        let obj = m.captures().iter().find(|c| c.index == obj_idx).unwrap();
        if obj.node.utf8_text(source.as_bytes()).unwrap_or("") != "console" {
            continue;
        }
        let report = m.captures().iter().find(|c| c.index == report_idx).unwrap();
        let start = report.node.start_position();
        out.push(Diagnostic {
            rule: "no-console",
            line: start.row + 1,
            column: start.column + 1,
            message: "unexpected console call".to_string(),
        });
    }
    out
}

fn no_explicit_any_rule(tree: &Tree, source: &str) -> Vec<Diagnostic> {
    let query = Query::new(
        &tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        "(predefined_type) @report",
    )
    .expect("invalid no-explicit-any query");
    let report_idx = query.capture_index_for_name("report").unwrap();

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
    let mut out = Vec::new();
    while let Some(m) = matches.next() {
        let report = m.captures().iter().find(|c| c.index == report_idx).unwrap();
        if report.node.utf8_text(source.as_bytes()).unwrap_or("") != "any" {
            continue;
        }
        let start = report.node.start_position();
        out.push(Diagnostic {
            rule: "no-explicit-any",
            line: start.row + 1,
            column: start.column + 1,
            message: "unexpected 'any' type".to_string(),
        });
    }
    out
}

/// ponytail: combining an `operator:` field constraint with another field constraint
/// (e.g. `left:`) on the same `binary_expression` pattern silently matches nothing in
/// this tree-sitter binding, even though the query compiles without error — confirmed
/// in isolation, see FINDINGS.md. So this matches on `left`/`right` alone and checks the
/// operator's text manually, same workaround as the other text-sensitive rules above.
fn no_redundant_boolean_comparison_rule(tree: &Tree, source: &str) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for field in ["left", "right"] {
        let query_src = format!("(binary_expression {field}: [(true) (false)]) @report");
        let query = Query::new(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(), &query_src)
            .expect("invalid no-redundant-boolean query");
        let report_idx = query.capture_index_for_name("report").unwrap();

        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
        while let Some(m) = matches.next() {
            let report = m.captures().iter().find(|c| c.index == report_idx).unwrap();
            let node = report.node;
            let operator = node
                .child_by_field_name("operator")
                .and_then(|op| op.utf8_text(source.as_bytes()).ok())
                .unwrap_or("");
            if !matches!(operator, "==" | "===" | "!=" | "!==") {
                continue;
            }
            let start = node.start_position();
            out.push(Diagnostic {
                rule: "no-redundant-boolean",
                line: start.row + 1,
                column: start.column + 1,
                message: "comparing to a boolean literal is redundant".to_string(),
            });
        }
    }
    out
}

/// Runs a tree-sitter query and reports one diagnostic per match at the `@report` capture.
fn query_rule(
    tree: &Tree,
    source: &str,
    rule: &'static str,
    message: &str,
    query_src: &str,
) -> Vec<Diagnostic> {
    let query = Query::new(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(), query_src)
        .unwrap_or_else(|e| panic!("invalid query for rule {rule}: {e}"));
    let report_idx = query
        .capture_index_for_name("report")
        .unwrap_or_else(|| panic!("rule {rule} query is missing a @report capture"));

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
    let mut out = Vec::new();
    while let Some(m) = matches.next() {
        if let Some(capture) = m.captures().iter().find(|c| c.index == report_idx) {
            let start = capture.node.start_position();
            out.push(Diagnostic {
                rule,
                line: start.row + 1,
                column: start.column + 1,
                message: message.to_string(),
            });
        }
    }
    out
}

/// ponytail: empty-block check needs a child count, which a query alone can't express.
fn no_empty_catch_rule(tree: &Tree, source: &str) -> Vec<Diagnostic> {
    let query = Query::new(
        &tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        "(catch_clause body: (statement_block) @body) @report",
    )
    .expect("invalid no-empty-catch query");
    let body_idx = query.capture_index_for_name("body").unwrap();

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
    let mut out = Vec::new();
    while let Some(m) = matches.next() {
        if let Some(capture) = m.captures().iter().find(|c| c.index == body_idx) {
            let body: Node = capture.node;
            if body.named_child_count() == 0 {
                let start = body.start_position();
                out.push(Diagnostic {
                    rule: "no-empty-catch",
                    line: start.row + 1,
                    column: start.column + 1,
                    message: "empty catch block silently swallows errors".to_string(),
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

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
