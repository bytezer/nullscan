//! Flags a function that sometimes returns a value but can also return without one.
//!
//! Conservative subset and deliberate limitation: this is **path-insensitive**. It reports
//! when the function contains at least one valued `return` and either:
//!
//! - also contains a bare `return;`, or
//! - its body can fall off the end (the last executing statement does not terminate).
//!
//! Returns inside nested functions are ignored, and loops/`switch` statements are not
//! proven terminating, so a function ending in `while (true) { return x; }` is reported
//! even though it cannot actually fall through.

use tree_sitter::Node;

use crate::Diagnostic;
use crate::control_flow::{
    collect_returns, is_function_like, last_executable_statement, non_comment_children,
    statement_terminates,
};
use crate::rule::FileContext;

use super::report_at;

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    walk(ctx.tree.root_node(), &mut out);
    out
}

fn walk(node: Node<'_>, out: &mut Vec<Diagnostic>) {
    if is_function_like(node) {
        check_function(node, out);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, out);
    }
}

fn check_function(node: Node<'_>, out: &mut Vec<Diagnostic>) {
    let Some(body) = node.child_by_field_name("body") else {
        return;
    };
    if body.kind() != "statement_block" {
        return;
    }

    let returns = collect_returns(body);
    let has_valued_return = returns
        .iter()
        .any(|statement| !non_comment_children(*statement).is_empty());
    if !has_valued_return {
        return;
    }

    let has_bare_return = returns
        .iter()
        .any(|statement| non_comment_children(*statement).is_empty());
    let falls_off_end =
        last_executable_statement(body).is_none_or(|last| !statement_terminates(last));

    if has_bare_return || falls_off_end {
        out.push(report_at(
            "consistent-return",
            node,
            "function returns a value on some paths but not on others",
        ));
    }
}
