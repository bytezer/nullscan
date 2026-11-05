# Rule catalog

Every entry in `RULES` (`nullscan-core/src/rule.rs`) has one row here. The registry is
the source of truth for IDs and descriptions; this document adds the human-facing
category and the closest upstream catalog equivalent.

Upstream identifiers below are SonarJS rule keys (the `S####` IDs used by
`eslint-plugin-sonarjs` and SonarQube) and/or ESLint rule names. Where no reliable
equivalent exists, the cell says so rather than guessing an ID. Category values are
`bug`, `code smell`, `metric`, or `duplication`.

| nullscan ID | Category | Source / equivalent catalog rule | Implementation status |
|---|---|---|---|
| `no-console` | code smell | SonarJS S106 `no-console`; ESLint `no-console` | Implemented (file-scoped) |
| `no-debugger` | code smell | SonarJS S1525 `no-debugger`; ESLint `no-debugger` | Implemented (file-scoped) |
| `no-var` | code smell | SonarJS S3504 `no-var`; ESLint `no-var` | Implemented (file-scoped) |
| `eqeqeq` | bug | SonarJS S1440 `eqeqeq`; ESLint `eqeqeq` | Implemented (file-scoped) |
| `no-explicit-any` | code smell | SonarJS S4204 `no-explicit-any`; typescript-eslint `no-explicit-any` | Implemented (file-scoped) |
| `no-empty-catch` | bug | SonarJS S2486 `no-ignored-exceptions`; ESLint `no-empty` (catch blocks) | Implemented (file-scoped) |
| `cognitive-complexity` | metric | SonarJS S3776 `cognitive-complexity`; no ESLint core equivalent (ESLint `complexity` is cyclomatic) | Implemented (file-scoped) |
| `no-duplicate-string` | code smell | SonarJS S1192 `no-duplicate-string` | Implemented (file-scoped) |
| `no-identical-conditions` | bug | SonarJS S1862 `no-identical-conditions`; ESLint `no-dupe-else-if` | Implemented (file-scoped) |
| `no-redundant-boolean` | code smell | SonarJS S1125 `no-redundant-boolean` | Implemented (file-scoped) |
| `prefer-immediate-return` | code smell | SonarJS S1488 `prefer-immediate-return` | Implemented (file-scoped) |
| `no-collapsible-if` | code smell | SonarJS S1066 `no-collapsible-if` | Implemented (file-scoped) |
| `duplicate-code` | duplication | SonarQube duplication metrics (`duplicated_blocks`); not a SonarJS rule | Implemented (project-scoped, CLI) |

## Adding a rule

1. Implement it under `nullscan-core/src/lang/typescript/rules/` as
   `pub fn run(&FileContext<'_>) -> Vec<Diagnostic>`.
2. Register it exactly once in `RULES` in `nullscan-core/src/rule.rs` with its
   `RuleMeta`, `RuleScope`, and runner (project-scoped rules use `run: None` plus an
   orchestrator).
3. Add `nullscan-core/tests/fixtures/<rule-id>/valid.ts` (or `.tsx`) and
   `invalid.ts` (or `.tsx`). The fixture harness at `nullscan-core/tests/rules.rs`
   discovers the directory from the rule ID and asserts that `valid.*` is clean for
   that rule while `invalid.*` produces at least one diagnostic. Other rules may fire
   on a fixture; only the rule named by the directory is checked.
4. Run `cargo test --workspace`. The harness fails any file-scoped registry rule that
   has no valid/invalid fixture pair, so a new rule cannot land without fixtures.
5. Add or update the row in this table.

Project-scoped rules (`duplicate-code`) are skipped by the per-file fixture harness and
need their own orchestration test instead.
