# Findings

Technical notes and gotchas discovered during development, so we don't repeat the
research (or the mistake) next time.

## tree-sitter: `#eq?`/`#match?` predicates aren't evaluated for you

`QueryCursor::matches` (crate `tree-sitter` 0.27, Rust binding) does **not** filter
matches using text predicates like `(#eq? @cap "something")`. The query returns every
structural match regardless of the predicate — you have to read the captured node's
text (`node.utf8_text(source.as_bytes())`) and filter manually in Rust.

**How it showed up:** the `no-explicit-any` rule used
`(predefined_type) @report (#eq? @report "any")` and flagged *every* primitive type
(`number`, `boolean`, `string`...), not just `any`. A test (`allows_concrete_types`)
caught it immediately.

**Practical rule:** use tree-sitter queries only for *structural* conditions (tree
shape, literal tokens like `"var"` or `"=="` inside the pattern). Any condition on a
captured node's *content/text* gets filtered in Rust after `cursor.matches(...)`, not
in the query predicate.

See `no_console_rule` and `no_explicit_any_rule` in `nullscan-core/src/lib.rs` for the
pattern to follow.
