use crate::cognitive_complexity::cognitive_complexity_rule;
use crate::Diagnostic;
use crate::rule::FileContext;

/// Wraps the metric implementation so the registry sees the same runner shape as every
/// other file rule. The scoring logic itself stays in `crate::cognitive_complexity`.
pub fn run(ctx: &FileContext<'_>) -> Vec<Diagnostic> {
    cognitive_complexity_rule(
        ctx.tree,
        ctx.source,
        ctx.config.cognitive_complexity_threshold,
    )
}
