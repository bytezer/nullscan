//! Flags a binding whose name also exists in an enclosing scope.
//!
//! Same-scope duplicates are left to `no-redeclare`. The check covers `let`/`const`/`var`,
//! parameters, catch parameters, imports, function declarations, and class declarations.
//! Because this is purely lexical, a binding in a nested function that runs much later is
//! still considered shadowing, matching the usual `no-shadow` semantics.

use crate::Diagnostic;
use crate::rule::FileContext;

use super::report_at_range;

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let Some(scopes) = ctx.scopes else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for binding in &scopes.bindings {
        let mut parent = scopes.scopes[binding.scope].parent;
        while let Some(scope_id) = parent {
            let shadowed = scopes.scopes[scope_id]
                .bindings
                .iter()
                .any(|&index| scopes.bindings[index].name == binding.name);
            if shadowed {
                out.extend(report_at_range(
                    ctx,
                    "no-shadow",
                    binding.offset,
                    binding.offset,
                    format!("rename '{}' to avoid shadowing the outer binding", binding.name),
                ));
                break;
            }
            parent = scopes.scopes[scope_id].parent;
        }
    }
    out.sort_by_key(|diagnostic| (diagnostic.line, diagnostic.column));
    out
}
