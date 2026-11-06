//! Flags a `let` declarator with an initializer whose names are never written after the
//! declaration.
//!
//! Conservative limits:
//! - Depends on the lexical scope graph's write counts, not control flow. A write in a
//!   branch that never executes still counts, so the rule prefers false negatives.
//! - Declarators without an initializer are skipped: `let x; x = 1;` is left alone.
//! - Destructuring is reported once per declarator.
//! - `for (... of ...)` / `for (... in ...)` bindings are not declarators with an
//!   initializer, so they are not reported.

use crate::rule::FileContext;
use crate::{BindingKind, Diagnostic};

use super::report_at_range;

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let Some(scopes) = ctx.scopes else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for declarator in &scopes.declarators {
        if declarator.kind != BindingKind::Let
            || !declarator.has_initializer
            || declarator.bindings.is_empty()
        {
            continue;
        }
        if declarator
            .bindings
            .iter()
            .any(|&index| scopes.bindings[index].writes > 0)
        {
            continue;
        }

        let names: Vec<String> = declarator
            .bindings
            .iter()
            .map(|&index| format!("'{}'", scopes.bindings[index].name))
            .collect();
        let message = if names.len() == 1 {
            format!("{} is never reassigned; use 'const' instead", names[0])
        } else {
            format!(
                "{} are never reassigned; use 'const' instead",
                names.join(", ")
            )
        };
        out.extend(report_at_range(
            ctx,
            "prefer-const",
            declarator.start_byte,
            declarator.end_byte,
            message,
        ));
    }
    out.sort_by_key(|diagnostic| (diagnostic.line, diagnostic.column));
    out
}
