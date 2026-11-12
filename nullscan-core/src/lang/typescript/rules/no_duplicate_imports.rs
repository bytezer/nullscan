//! Flags multiple `import` declarations from the same module in one file.
//!
//! The first import is kept and every later import from the same source string is
//! reported. `export ... from` declarations are ignored, as are dynamic `import()`
//! expressions. Module comparison uses the exact source string, including quotes.
//!
//! Type-only imports (`import type { T } from "m"`) are tracked apart from value imports,
//! as in eslint-plugin-import `no-duplicates`: one `import type` plus one value import
//! from the same module is the usual shape under `consistent-type-imports`. A second
//! import of the same kind is still reported. Inline `import { type T, v }` counts as a
//! value import.

use std::collections::HashSet;

use tree_sitter::Node;

use crate::Diagnostic;
use crate::rule::FileContext;

use super::{node_text, report_at};

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let mut seen: HashSet<(bool, &str)> = HashSet::new();
    let mut out = Vec::new();
    walk(ctx.tree.root_node(), ctx.source, &mut seen, &mut out);
    out
}

fn walk<'a>(
    node: Node<'a>,
    source: &'a str,
    seen: &mut HashSet<(bool, &'a str)>,
    out: &mut Vec<Diagnostic>,
) {
    if node.kind() == "import_statement" {
        if let Some(module) = node.child_by_field_name("source") {
            let module_text = node_text(module, source);
            if !seen.insert((is_type_only(node), module_text)) {
                out.push(report_at(
                    "no-duplicate-imports",
                    node,
                    format!("module {module_text} is imported more than once; merge it with the earlier import"),
                ));
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, source, seen, out);
    }
}

/// Whether the statement is `import type ...` (the `type` keyword sits directly under it).
fn is_type_only(import: Node<'_>) -> bool {
    let mut cursor = import.walk();
    import.children(&mut cursor).any(|child| child.kind() == "type")
}
