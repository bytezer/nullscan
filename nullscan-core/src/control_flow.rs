//! Small, reusable control-flow helpers.
//!
//! This is deliberately **not** a control-flow graph. The helpers answer local,
//! syntactic questions about statements (does this statement always leave its block?
//! which returns belong to this function?) so the reachability rules can share one
//! conservative definition instead of re-implementing tree walks.
//!
//! Termination coverage is intentionally small: `return`, `throw`, `break`,
//! `continue`, a block whose last executing statement terminates, and an `if`/`else`
//! where both branches terminate. `try`, `switch`, and loops are not analysed.

use tree_sitter::Node;

/// Function-like nodes whose bodies form a separate return scope.
pub(crate) const FUNCTION_KINDS: &[&str] = &[
    "function_declaration",
    "function_signature",
    "generator_function_declaration",
    "function_expression",
    "generator_function",
    "arrow_function",
    "method_definition",
    "method_signature",
    "abstract_method_signature",
];

const LOOP_KINDS: &[&str] = &[
    "for_statement",
    "for_in_statement",
    "while_statement",
    "do_statement",
];

/// Named children with comments and empty statements removed.
fn meaningful_children<'tree>(node: Node<'tree>) -> Vec<Node<'tree>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|child| !matches!(child.kind(), "comment" | "empty_statement"))
        .collect()
}

/// The statements of a statement list: a program, a block, or a switch case body.
///
/// Comments and empty statements are not meaningful statements and are skipped.
pub(crate) fn statements<'tree>(node: Node<'tree>) -> Vec<Node<'tree>> {
    match node.kind() {
        "program" | "statement_block" => meaningful_children(node),
        "switch_case" | "switch_default" => {
            let value = node.child_by_field_name("value");
            meaningful_children(node)
                .into_iter()
                .filter(|child| {
                    value.is_none_or(|value| {
                        child.start_byte() != value.start_byte()
                            || child.end_byte() != value.end_byte()
                    })
                })
                .collect()
        }
        _ => Vec::new(),
    }
}

/// Last meaningful statement of a block, ignoring comments and empty statements.
pub(crate) fn last_statement(node: Node<'_>) -> Option<Node<'_>> {
    statements(node).pop()
}

/// Declarations that are hoisted and therefore do not make later code unreachable.
pub(crate) fn is_hoisted_declaration(node: Node<'_>) -> bool {
    matches!(
        node.kind(),
        "function_declaration" | "generator_function_declaration" | "function_signature"
    )
}

/// Last statement that actually executes when control reaches the end of the block.
///
/// Hoisted function declarations are skipped, so `{ return; function f() {} }` still
/// terminates. This intentionally does not model declarations as runtime statements.
pub(crate) fn last_executable_statement(node: Node<'_>) -> Option<Node<'_>> {
    statements(node)
        .into_iter()
        .rev()
        .find(|statement| !is_hoisted_declaration(*statement))
}

/// Whether `node` is a function-like node. Used to keep per-function analysis from
/// descending into nested functions.
pub(crate) fn is_function_like(node: Node<'_>) -> bool {
    FUNCTION_KINDS.contains(&node.kind())
}

pub(crate) fn is_loop(node: Node<'_>) -> bool {
    LOOP_KINDS.contains(&node.kind())
}

/// Statements of a loop/function body, treating a single non-block statement as a
/// one-statement list.
pub(crate) fn body_statements<'tree>(node: Node<'tree>) -> Vec<Node<'tree>> {
    if matches!(node.kind(), "statement_block" | "program") {
        statements(node)
    } else {
        vec![node]
    }
}

/// Whether a statement always transfers control away from the rest of its block.
pub(crate) fn statement_terminates(node: Node<'_>) -> bool {
    match node.kind() {
        "return_statement" | "throw_statement" | "break_statement" | "continue_statement" => true,
        "statement_block" | "program" => {
            last_executable_statement(node).is_some_and(statement_terminates)
        }
        "if_statement" => if_terminates(node),
        _ => false,
    }
}

fn if_terminates(node: Node<'_>) -> bool {
    let (Some(consequence), Some(alternative)) = (
        node.child_by_field_name("consequence"),
        node.child_by_field_name("alternative"),
    ) else {
        return false;
    };
    statement_terminates(consequence) && else_terminates(alternative)
}

fn else_terminates(node: Node<'_>) -> bool {
    if node.kind() == "else_clause" {
        meaningful_children(node)
            .first()
            .is_some_and(|branch| statement_terminates(*branch))
    } else {
        statement_terminates(node)
    }
}

/// Returns directly inside `body`, skipping nested function-like nodes.
pub(crate) fn collect_returns<'tree>(body: Node<'tree>) -> Vec<Node<'tree>> {
    let mut out = Vec::new();
    collect_returns_inner(body, &mut out);
    out
}

fn collect_returns_inner<'tree>(node: Node<'tree>, out: &mut Vec<Node<'tree>>) {
    if is_function_like(node) {
        return;
    }
    if node.kind() == "return_statement" {
        out.push(node);
        return;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_returns_inner(child, out);
    }
}

/// Named children of a node, ignoring comments, for rules that need the direct contents
/// of a returned/throw expression.
pub(crate) fn non_comment_children<'tree>(node: Node<'tree>) -> Vec<Node<'tree>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|child| child.kind() != "comment")
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Language;

    fn parse(source: &str) -> tree_sitter::Tree {
        Language::TypeScript
            .parse(source)
            .expect("source should parse")
    }

    fn terminates(source: &str) -> bool {
        let tree = parse(source);
        let function = tree.root_node().named_child(0).unwrap();
        let body = function.child_by_field_name("body").unwrap();
        let statement = statements(body)[0];
        statement_terminates(statement)
    }

    #[test]
    fn simple_jumps_terminate() {
        assert!(terminates("function f() { return; }"));
        assert!(terminates("function f() { return 1; }"));
        assert!(terminates("function f() { throw new Error(); }"));
        // Loops are not part of the termination model.
        assert!(!terminates("function f() { while (true) { break; } }"));
        assert!(!terminates("function f() { for (;;) { continue; } }"));
    }

    #[test]
    fn blocks_and_if_else_terminate_through_their_last_statement() {
        assert!(terminates("function f(a) { { return 1; } }"));
        assert!(terminates(
            "function f(a) { if (a) { return 1; } else { throw 2; } }"
        ));
        assert!(!terminates("function f(a) { if (a) { return 1; } }"));
        assert!(!terminates(
            "function f(a) { if (a) { return 1; } else { log(a); } }"
        ));
    }

    #[test]
    fn last_statement_ignores_comments_and_empty_statements() {
        let tree = parse("function f() { return 1; ; /* done */ }");
        let function = tree.root_node().named_child(0).unwrap();
        let body = function.child_by_field_name("body").unwrap();
        let last = last_statement(body).unwrap();
        assert_eq!(last.kind(), "return_statement");
    }

    #[test]
    fn last_executable_statement_skips_hoisted_functions() {
        let tree = parse("function f() { return 1; function g() {} }");
        let function = tree.root_node().named_child(0).unwrap();
        let body = function.child_by_field_name("body").unwrap();
        let last = last_executable_statement(body).unwrap();
        assert_eq!(last.kind(), "return_statement");
    }

    #[test]
    fn collect_returns_does_not_descend_into_nested_functions() {
        let tree =
            parse("function f() { if (a) { return 1; } const g = () => { return 2; }; return 3; }");
        let function = tree.root_node().named_child(0).unwrap();
        let body = function.child_by_field_name("body").unwrap();
        let returns = collect_returns(body);
        assert_eq!(returns.len(), 2);
    }
}
