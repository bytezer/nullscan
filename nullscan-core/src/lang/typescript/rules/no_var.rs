use crate::Diagnostic;
use crate::rule::FileContext;

use super::query_rule;

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    query_rule(
        ctx,
        "no-var",
        "unexpected 'var', use 'let' or 'const'",
        r#"
            (variable_declaration "var") @report
        "#,
    )
}
