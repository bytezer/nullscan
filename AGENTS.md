# AGENTS.md

`nullscan` is a Rust workspace for a TypeScript static analyzer: a reusable core crate plus a CLI. Read `README.md` for usage and `FINDINGS.md` for the full tree-sitter investigation notes.

## Commands

```sh
cargo test --workspace                       # full suite (~20s, 129 tests)
cargo test -p nullscan-core flags_explicit_any   # one test by name
cargo run -p nullscan-cli -- path/to/src/    # analyze files/dirs
cargo run -p nullscan-cli -- src/ --format sarif
cargo run -p nullscan-cli -- src/ --config path/to/nullscan.toml
```

There is no CI workflow, rustfmt/clippy config, or pre-commit hook; `cargo test --workspace` is the verification step. Edition 2024 requires a recent toolchain (tested on 1.98).

## Commits

- Use Conventional Commits: `type(scope): summary`, with types `feat`, `fix`, `refactor`, `perf`, `docs`, `test`, `build`, `ci`, `chore`, `revert`.
- Scope names the affected area, for example `feat(rules): add no-eval` or `fix(cli): keep output order deterministic`.
- Summary: English, imperative, lowercase after the colon, <= 72 chars, no trailing period. Body explains why, wrapped at 72.
- One logical change per commit; keep refactors separate from behavior changes. Land a rule together with its fixture pair and its `RULES.md` row in a single commit.
- Commits before this convention use a free-form style; do not rewrite history.

## Code style

- All identifiers, comments, doc comments, log/error messages and test names are in English.
- Keep functions atomic: one job, roughly <= 40 lines, <= 4 parameters, nesting <= 3. Extract helpers rather than growing `run()` functions.
- Group code by domain: one module per rule under `lang/typescript/rules/`, and shared helpers in the module that owns them (`rule.rs`, `scope.rs`, `control_flow.rs`) instead of a generic `utils` module.

## Layout and flow

- `nullscan-core/` — tree-sitter parsing, the rule registry, per-file rules, cognitive complexity, cross-file duplication. No `serde` in this crate; `Config` is a plain struct.
- `nullscan-core/src/rule.rs` — **single source of truth** for rule metadata: `RuleMeta`, `RuleScope`, `Rule`, `FileContext`, and the `RULES` registry (including the project-scoped `duplicate-code`).
- `RULES.md` — human-facing catalog, one row per registry rule: category, closest SonarJS/ESLint equivalent, and implementation status. Update it when adding a rule.
- `nullscan-core/tests/rules.rs` + `nullscan-core/tests/fixtures/<rule-id>/` — fixture-based regression harness. It discovers fixture directories dynamically and checks `valid.ts|tsx` (zero diagnostics for that rule) and `invalid.ts|tsx` (at least one). Other rules may fire on a fixture; only the rule named by the directory is asserted. Project-scoped rules are skipped.
- `nullscan-core/src/scope.rs` — owned per-file lexical scope graph (`ScopeGraph`): module/function/block/catch/class/loop/switch scopes, binding kinds, and references with read/write counts. No tree-sitter `Node` values are stored. Built lazily by `analyze` only when an enabled file rule has `requires_scope: true`.
- `nullscan-core/src/control_flow.rs` — small reusable statement helpers, deliberately not a CFG: `statement_terminates`, `last_statement`/`last_executable_statement`, `statements`, `body_statements`, `is_function_like`, `is_loop`, and `collect_returns` (skipping nested functions). The reachability/dataflow rules share these instead of duplicating tree walks; termination coverage is limited to return/throw/break/continue, terminating blocks, and `if`/`else`. Unit tests for the helpers live in the module.
- `nullscan-core/src/lang/typescript/rules/` — one module per per-file rule; each exposes `pub fn run(&FileContext) -> Vec<Diagnostic>`. Shared query helper in `query.rs`. Scope-dependent rules read `ctx.scopes` and return early when it is `None`.
- `nullscan-core/src/cognitive_complexity.rs` and `duplication.rs` — metric and clone-detection implementations, kept at the crate root; the TypeScript rule module only wraps them.
- `nullscan-cli/` — clap argument parsing, `nullscan.toml` loading/deserialization, file walking, output formatting (text/SARIF), exit codes.
- `nullscan_core::analyze()` (in `lib.rs`) parses once, builds one `FileContext { tree, source, config, scopes }`, and runs every enabled `RuleScope::File` entry from `RULES` in registry order. It builds the `ScopeGraph` only when at least one enabled file rule sets `requires_scope: true`, then passes it as `ctx.scopes`; otherwise that field is `None`. The public `analyze(source, language, config)` signature is unchanged. TS and TSX share one rule set, so queries must be built with `tree.language()` rather than a hard-coded grammar.
- Cross-file `duplicate-code` is registered in `RULES` as `RuleScope::Project` with `run: None`; the CLI orchestrates it in `find_duplicates` because it needs all sources. Rules needing whole-project context do not belong in `analyze()`.
- The CLI's SARIF catalog and `rules.disabled` validation both read `nullscan_core::RULES`; there is no duplicate metadata list in the CLI.
- Exit code is 1 if any diagnostic or duplicate is found, else 0. `rayon` parallel maps collect in input order, so output order stays deterministic; preserve that when touching the pipeline.

## Adding or changing a rule

- Add the implementation under `nullscan-core/src/lang/typescript/rules/` (one file per rule, `pub fn run(&FileContext<'_>) -> Vec<Diagnostic>`).
- Register it **exactly once** in `RULES` in `nullscan-core/src/rule.rs`, with its `RuleMeta` (id + description), `RuleScope`, `requires_scope`, and runner:
  - file-scoped rules: `RuleScope::File`, `requires_scope: false`, and `run: Some(rules::<name>::run)`;
  - scope-dependent file rules: same, but `requires_scope: true`, and read `ctx.scopes` (returning early if it is `None`);
  - project-scoped rules: `RuleScope::Project`, `requires_scope: false`, and `run: None`, plus their own orchestrator.
- The registry is the only place the CLI reads rule IDs/descriptions from, so no `RULE_DESCRIPTIONS` update is needed (or possible) in the CLI.
- Add `nullscan-core/tests/fixtures/<rule-id>/valid.ts` (or `.tsx`) and `invalid.ts` (or `.tsx`). The fixture harness fails any file-scoped registry rule without a pair, so this is part of landing a rule, not optional cleanup. Use `.tsx` for at least one pair when the rule should be guarded under TSX.
- Add the rule's row to `RULES.md` (category, upstream equivalent, status).
- Entry order in `RULES` is the diagnostic-emission order for file rules. Existing tests assert exact rule vectors; insert a new rule where its diagnostics should appear rather than reordering existing ones.
- For a new threshold, add it to `nullscan_core::Config` defaults **and** `FileThresholdsConfig` in `nullscan-cli/src/config.rs` (kebab-case `#[serde(rename = ...)]`).
- Structural rules with a single `@report` capture should use the shared `query_rule` helper in `rules/query.rs`. Anything that inspects captured text or child counts needs a hand-written rule.
- **tree-sitter traps** (details and repros in `FINDINGS.md`):
  - `QueryCursor::matches` does not evaluate `#eq?`/`#match?`; filter `node.utf8_text(...)` in Rust.
  - Combining `operator:` with another field (`left:`/`right:`) silently matches nothing in this binding. Match the other field and read the operator text in Rust.
- Scope-dependent rules read the owned `ScopeGraph` in `scope.rs`; never store tree-sitter `Node` values in it. Scope unit tests (function vs block scoping, shadowing, hoisting, parameters, imports, destructuring, read/write counts, globals, loops/switches) live in `scope.rs`; update them when resolution changes.
- Reachability rules use `control_flow.rs` (and its unit tests) rather than building a CFG. `no-unused-collection` is the only Phase 4 rule that sets `requires_scope: true`; the other dataflow rules are syntax-only by design.
- The Phase 5 security rules (`no-eval`, `no-implied-eval`, `no-new-func`, `no-script-url`, `hardcoded-secrets`) are syntax-only and deliberately avoid taint analysis, type information, and full dataflow. `hardcoded-secrets` is a name/value heuristic, not a secret scanner; see `RULES.md` for the full limitation list. The shared `static_string_text` helper in `rules/query.rs` recognizes `string` literals and substitution-free `template_string` nodes.
- Unit tests stay centrally in `nullscan-core/src/lang/typescript.rs` and assert exact rule vectors; cognitive complexity asserts exact scores. Registry sanity tests (unique IDs, non-empty descriptions, `duplicate-code`, stable ordering, `requires_scope`) live in `rule.rs`. Fixture-based regression tests live in `nullscan-core/tests/rules.rs`. Update the relevant layer when behavior changes.

## CLI/file-walking notes

- Supported inputs: `.ts`, `.mts`, `.cts`, `.tsx`; JS is not supported.
- Directory walks skip dot-directories and `SKIPPED_DIRS` (`node_modules`, `dist`, `build`, `.next`, `out`) to avoid scanning generated output.
- Default config is `./nullscan.toml` relative to the process CWD, not the target path.
