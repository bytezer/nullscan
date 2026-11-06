use crate::Diagnostic;
use crate::rule::FileContext;

use super::query_rule;

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    query_rule(
        ctx,
        "no-debugger",
        "remove this 'debugger' statement before committing",
        r#"
            (debugger_statement) @report
        "#,
    )
}
