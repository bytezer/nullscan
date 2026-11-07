//! Flags a `let`, `const`, or `class` binding used earlier in the same scope than its
//! declaration.
//!
//! Deliberate limitations:
//! - Only the same lexical scope is checked. A reference in a nested block or function is
//!   not reported, even if that nested code could run before the declaration.
//! - `var` and function declarations are hoisted, so earlier references to them are
//!   accepted.
//! - Globals (unresolved names) are ignored.
//! - Type-only references (`const x: Foo = ...` before `interface Foo {}`; the `K` in a
//!   `typeof K` type query; `foo<Bar>()` before `Bar` exists) are ignored, because types
//!   are erased at runtime and cannot hit a temporal dead zone.
//!
//! This conservative shape avoids reporting references inside functions that only execute
//! later, at the cost of missing some genuine temporal-dead-zone uses.

use crate::rule::FileContext;
use crate::{BindingKind, Diagnostic};

use super::report_at_range;

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let Some(scopes) = ctx.scopes else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for reference in &scopes.references {
        if reference.is_type_position {
            continue;
        }
        let Some(binding_index) = reference.binding else {
            continue;
        };
        let binding = &scopes.bindings[binding_index];
        if !matches!(
            binding.kind,
            BindingKind::Let | BindingKind::Const | BindingKind::Class
        ) {
            continue;
        }
        if binding.scope != reference.scope || reference.offset >= binding.offset {
            continue;
        }
        out.extend(report_at_range(
            ctx,
            "no-use-before-define",
            reference.offset,
            reference.offset,
            format!("move the declaration of '{}' before this use", reference.name),
        ));
    }
    out.sort_by_key(|diagnostic| (diagnostic.line, diagnostic.column));
    out
}
