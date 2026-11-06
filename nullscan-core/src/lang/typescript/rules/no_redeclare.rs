//! Flags a second binding with the same name in one scope.
//!
//! This covers `let`/`const`/`var`/function/class/import conflicts, plus parameters and
//! catch parameters. tree-sitter is error-tolerant, so syntax-invalid combinations that a
//! real TypeScript compiler would reject can still reach this rule and be reported.
//!
//! A declaration is compared against every earlier declaration in the *same* scope;
//! shadowing across scopes belongs to `no-shadow`.

use std::collections::HashSet;

use crate::Diagnostic;
use crate::rule::FileContext;

use super::report_at_range;

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let Some(scopes) = ctx.scopes else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for scope in &scopes.scopes {
        let mut seen = HashSet::new();
        for &binding_index in &scope.bindings {
            let binding = &scopes.bindings[binding_index];
            if !seen.insert(binding.name.as_str()) {
                out.extend(report_at_range(
                    ctx,
                    "no-redeclare",
                    binding.offset,
                    binding.offset,
                    format!("'{}' is already declared in this scope", binding.name),
                ));
            }
        }
    }
    out.sort_by_key(|diagnostic| (diagnostic.line, diagnostic.column));
    out
}
