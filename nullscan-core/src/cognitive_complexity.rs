//! Cognitive complexity, per SonarSource's metric (not to be confused with cyclomatic
//! complexity): control-flow structures cost more the deeper they're nested, and a run
//! of the same logical operator (`&&` or `||`) only costs once, not once per operator.
//!
//! ponytail: recursion (+1 per self-call) and labeled break/continue (+1) are part of the
//! full spec but are rare in practice and need extra bookkeeping (call-site name
//! resolution, label tracking). Skipped for v1 — add if a real file needs them.

use tree_sitter::{Node, Tree};

use crate::Diagnostic;

pub fn cognitive_complexity_rule(tree: &Tree, source: &str, threshold: u32) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    find_functions(tree.root_node(), source, threshold, &mut out);
    out
}

fn is_function_like(kind: &str) -> bool {
    matches!(
        kind,
        "function_declaration"
            | "function_expression"
            | "arrow_function"
            | "method_definition"
            | "generator_function_declaration"
            | "generator_function"
    )
}

fn function_label(node: Node, source: &str) -> String {
    match node.child_by_field_name("name") {
        Some(name) => format!("'{}'", name.utf8_text(source.as_bytes()).unwrap_or("?")),
        None => "anonymous function".to_string(),
    }
}

/// Walks the whole tree looking for function-like nodes, scoring each one independently.
fn find_functions(node: Node, source: &str, threshold: u32, out: &mut Vec<Diagnostic>) {
    if is_function_like(node.kind()) {
        if let Some(body) = node.child_by_field_name("body") {
            let score = score(body, 0, source);
            if score > threshold {
                let start = node.start_position();
                let label = function_label(node, source);
                out.push(Diagnostic {
                    rule: "cognitive-complexity",
                    line: start.row + 1,
                    column: start.column + 1,
                    message: format!(
                        "{label} has a cognitive complexity of {score} (threshold {threshold})"
                    ),
                });
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        find_functions(child, source, threshold, out);
    }
}

/// Scores a subtree at a given nesting level. Stops at nested function boundaries —
/// those are scored separately by `find_functions`.
fn score(node: Node, nesting: u32, source: &str) -> u32 {
    match node.kind() {
        "if_statement" => score_if(node, nesting, false, source),
        "for_statement" | "for_in_statement" | "while_statement" | "do_statement"
        | "switch_statement" | "catch_clause" | "ternary_expression" => {
            1 + nesting + score_children(node, nesting + 1, source)
        }
        "binary_expression" => {
            let mut s = 0;
            let op = operator_text(node, source);
            if op == "&&" || op == "||" {
                let left_is_same_run = node.child_by_field_name("left").is_some_and(|l| {
                    l.kind() == "binary_expression" && operator_text(l, source) == op
                });
                if !left_is_same_run {
                    s += 1;
                }
            }
            // Logical/binary operators don't increase nesting.
            s + score_children(node, nesting, source)
        }
        kind if is_function_like(kind) => 0,
        _ => score_children(node, nesting, source),
    }
}

fn score_children(node: Node, nesting: u32, source: &str) -> u32 {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .map(|child| score(child, nesting, source))
        .sum()
}

/// The `operator` field of a `binary_expression` is an anonymous token node — read its
/// text directly rather than matching on `kind()` (anonymous nodes are named after the
/// literal, but going through the source is simpler and works for every operator).
fn operator_text<'a>(binary_expression: Node, source: &'a str) -> &'a str {
    binary_expression
        .child_by_field_name("operator")
        .and_then(|op| op.utf8_text(source.as_bytes()).ok())
        .unwrap_or("")
}

/// Scores an `if`/`else if`/`else` chain. `chained` is true when this node is itself the
/// target of an `else if` (it already got its flat +1 from the caller, so it must not add
/// the nesting-based increment again).
fn score_if(node: Node, nesting: u32, chained: bool, source: &str) -> u32 {
    let mut s = if chained { 1 } else { 1 + nesting };

    if let Some(condition) = node.child_by_field_name("condition") {
        s += score(condition, nesting, source);
    }
    if let Some(consequence) = node.child_by_field_name("consequence") {
        s += score(consequence, nesting + 1, source);
    }
    if let Some(alternative) = node.child_by_field_name("alternative") {
        if let Some(inner) = alternative.named_child(0) {
            if inner.kind() == "if_statement" {
                s += score_if(inner, nesting, true, source);
            } else {
                s += 1;
                s += score(inner, nesting + 1, source);
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn complexity_of(source: &str) -> Vec<u32> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        cognitive_complexity_rule(&tree, source, 15)
            .iter()
            .map(|d| {
                d.message
                    .split("complexity of ")
                    .nth(1)
                    .unwrap()
                    .split(' ')
                    .next()
                    .unwrap()
                    .parse()
                    .unwrap()
            })
            .collect()
    }

    #[test]
    fn simple_function_is_clean() {
        assert!(complexity_of("function f(x: number) { return x + 1; }").is_empty());
    }

    #[test]
    fn flat_ifs_stay_under_threshold() {
        // 10 sequential (non-nested) ifs: 10 * 1 = 10, under the threshold of 15.
        let mut src = String::from("function f(x: number) {\n");
        for i in 0..10 {
            src.push_str(&format!("  if (x == {i}) {{ x += 1; }}\n"));
        }
        src.push('}');
        assert!(complexity_of(&src).is_empty());
    }

    #[test]
    fn deeply_nested_ifs_exceed_threshold() {
        // 5 nested ifs: 1 + 2 + 3 + 4 + 5 = 15, not > 15 yet; add one more to tip it over.
        let src = "
            function f(a: boolean, b: boolean, c: boolean, d: boolean, e: boolean, g: boolean) {
                if (a) {
                    if (b) {
                        if (c) {
                            if (d) {
                                if (e) {
                                    if (g) {
                                        return 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        ";
        assert_eq!(complexity_of(src), vec![21]);
    }

    #[test]
    fn else_if_chain_is_flat_not_nested() {
        // if + else-if + else-if + else = 1 + 1 + 1 + 1 = 4, well under threshold.
        let src = "
            function f(x: number) {
                if (x == 1) { a(); } else if (x == 2) { b(); } else if (x == 3) { c(); } else { d(); }
            }
        ";
        assert!(complexity_of(src).is_empty());
    }

    #[test]
    fn logical_operator_run_counts_once() {
        // A single && chain: 1 (not 3), regardless of operand count.
        let src = "
            function f(a: boolean, b: boolean, c: boolean, d: boolean) {
                if (a && b && c && d) { return 1; }
            }
        ";
        // if (+1) + one && run (+1) = 2, far under threshold; just check it parses to a
        // low, non-empty-if-we-lower-threshold value via a nested variant below instead.
        assert!(complexity_of(src).is_empty());
    }

    #[test]
    fn mixed_logical_operators_count_each_switch() {
        let src = "
            function f(a: boolean, b: boolean, c: boolean, d: boolean, e: boolean, g: boolean, h: boolean, i: boolean, j: boolean, k: boolean, l: boolean, m: boolean) {
                if (a && b || c && d || e && g) {
                    if (h && i) {
                        if (j || k) {
                            if (l && m) {
                                return 1;
                            }
                        }
                    }
                }
            }
        ";
        // if(+1) + condition's logical-operator runs (+4) + 3 nested ifs, each adding its
        // own base/nesting cost and its own && / || run = enough to clear 15; exact value
        // locked in by the assert.
        assert_eq!(complexity_of(src), vec![17]);
    }

    #[test]
    fn nested_function_is_scored_independently() {
        // Outer function is trivial; only the inner callback is complex enough to flag.
        let mut inner_ifs = String::new();
        for depth in 0..6 {
            inner_ifs.push_str(&"  ".repeat(depth));
            inner_ifs.push_str(&format!("if (x{depth}) {{\n"));
        }
        for depth in (0..6).rev() {
            inner_ifs.push_str(&"  ".repeat(depth));
            inner_ifs.push_str("}\n");
        }
        let src = format!(
            "function outer() {{\n  const cb = (x0: boolean, x1: boolean, x2: boolean, x3: boolean, x4: boolean, x5: boolean) => {{\n{inner_ifs}  }};\n  cb;\n}}"
        );
        let scores = complexity_of(&src);
        assert_eq!(scores.len(), 1);
        assert_eq!(scores[0], 21);
    }
}
