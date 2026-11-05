use crate::Diagnostic;
use crate::rule::FileContext;

use super::query_rule;

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    query_rule(
        ctx,
        "no-debugger",
        "unexpected 'debugger' statement",
        r#"
            (debugger_statement) @report
        "#,
    )
}
