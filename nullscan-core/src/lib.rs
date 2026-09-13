mod cognitive_complexity;

use tree_sitter::{Node, Parser, Query, QueryCursor, StreamingIterator, Tree};

use cognitive_complexity::cognitive_complexity_rule;

pub struct Diagnostic {
    pub rule: &'static str,
    pub line: usize,
    pub column: usize,
    pub message: String,
}

/// Parses TypeScript source and runs all built-in rules against it.
pub fn analyze(source: &str) -> Vec<Diagnostic> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
        .expect("failed to load TypeScript grammar");
    let tree = parser.parse(source, None).expect("parse failed");

    let mut diagnostics = Vec::new();
    diagnostics.extend(no_console_rule(&tree, source));
    diagnostics.extend(query_rule(&tree, source, "no-debugger", "unexpected 'debugger' statement", r#"
        (debugger_statement) @report
    "#));
    diagnostics.extend(query_rule(&tree, source, "no-var", "unexpected 'var', use 'let' or 'const'", r#"
        (variable_declaration "var") @report
    "#));
    diagnostics.extend(query_rule(&tree, source, "eqeqeq", "expected '===' and instead saw '=='", r#"
        (binary_expression operator: "==") @report
    "#));
    diagnostics.extend(query_rule(&tree, source, "eqeqeq", "expected '!==' and instead saw '!='", r#"
        (binary_expression operator: "!=") @report
    "#));
    diagnostics.extend(no_explicit_any_rule(&tree, source));
    diagnostics.extend(no_empty_catch_rule(&tree, source));
    diagnostics.extend(cognitive_complexity_rule(&tree, source));
    diagnostics
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
        analyze(source).iter().map(|d| d.rule).collect()
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
}
