use crate::Diagnostic;
use crate::rule::FileContext;

use super::query_rule;

pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    let mut out = query_rule(
        ctx,
        "eqeqeq",
        "expected '===' and instead saw '=='",
        r#"
            (binary_expression operator: "==") @report
        "#,
    );
    out.extend(query_rule(
        ctx,
        "eqeqeq",
        "expected '!==' and instead saw '!='",
        r#"
            (binary_expression operator: "!=") @report
        "#,
    ));
    out
}
