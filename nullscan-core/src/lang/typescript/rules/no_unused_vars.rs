//! Flags unused imports and unused local variables.
//!
//! Conservative limits:
//! - Module-level `let`/`const`/`var` declarations are never reported, so exported and
//!   otherwise externally consumed bindings are not flagged.
//! - Function/class declarations and parameters are not checked.
//! - A binding used only in an `export { ... }` specifier is treated as unused, because
//!   export specifier names are not references in the scope model (matching the
//!   instruction to count only real value/type uses).
//! - Names prefixed with `_` are ignored, and TS type-position uses count as reads.
//! - Destructured bindings are checked one by one.

use crate::rule::FileContext;
use crate::{BindingKind, Diagnostic, ScopeKind};

use super::report_at_range;

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let Some(scopes) = ctx.scopes else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for binding in &scopes.bindings {
        if binding.name.starts_with('_') || binding.reads > 0 {
            continue;
        }

        let report = match binding.kind {
            BindingKind::Import => true,
            BindingKind::Var | BindingKind::Let | BindingKind::Const => {
                scopes.scopes[binding.scope].kind != ScopeKind::Module
            }
            BindingKind::Function
            | BindingKind::Class
            | BindingKind::Param
            | BindingKind::CatchParam => false,
        };

        if report {
            out.extend(report_at_range(
                ctx,
                "no-unused-vars",
                binding.offset,
                binding.offset,
                format!("'{}' is declared but never used; remove it or use it", binding.name),
            ));
        }
    }
    out.sort_by_key(|diagnostic| (diagnostic.line, diagnostic.column));
    out
}
