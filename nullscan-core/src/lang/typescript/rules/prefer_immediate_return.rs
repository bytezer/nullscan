use tree_sitter::Node;

use crate::Diagnostic;
use crate::rule::FileContext;

/// Flags `const x = expr; return x;` — the variable adds nothing over `return expr;`.
///
/// ponytail: only the single-declarator, immediately-adjacent-statement case is
/// checked (no data-flow analysis for "declared, then only used in the return despite
/// other statements in between"). That broader case needs real liveness analysis.
pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    walk_for_immediate_returns(ctx.tree.root_node(), ctx.source, &mut out);
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
    if declarator.kind() != "variable_declarator"
        || declarator.child_by_field_name("value").is_none()
    {
        return;
    }
    let Some(name_node) = declarator.child_by_field_name("name") else {
        return;
    };
    if name_node.kind() != "identifier" {
        return;
    }
    let Some(returned) = ret.named_child(0) else {
        return;
    };
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
        message: format!(
            "'{name}' is declared only to be returned immediately; return the expression directly"
        ),
    });
}
