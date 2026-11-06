# AGENTS.md

`nullscan` is a Rust workspace for a TypeScript static analyzer: a reusable core crate, a shared workspace crate, a CLI, and an LSP server. Read `README.md` for usage and `FINDINGS.md` for the full tree-sitter investigation notes.

## Commands

```sh
cargo test --workspace                       # full suite (~30s, 196 tests)
cargo test -p nullscan-core flags_explicit_any   # one test by name
cargo run -p nullscan -- path/to/src/         # analyze files/dirs
cargo run -p nullscan -- src/ --format sarif
cargo run -p nullscan -- src/ --format json
cargo run -p nullscan -- src/ --config path/to/nullscan.toml
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
- `nullscan-core/src/text.rs` — source-position helpers shared by consumers that speak UTF-16, chiefly `byte_col_to_utf16(line, byte_col)`. It converts a 0-based byte column on one line into a 0-based UTF-16 code-unit column, clamping out-of-range or mid-character columns.
- `Diagnostic` (in `nullscan-core/src/lib.rs`) carries `rule`, `severity`, `line`/`column`, `end_line`/`end_column` and `message`. Positions are 1-based byte positions; `Severity` is ordered `Error < Warning < Info` and exposes `as_str()` for text, SARIF and JSON. `rule::severity_of(rule_id)` resolves a registry id to its level and is re-exported from the crate root.
- `nullscan-core/src/rule.rs` — **single source of truth** for rule metadata: `RuleMeta`, `RuleScope`, `Rule`, `FileContext`, and the `RULES` registry (including the project-scoped `duplicate-code`).
- `RULES.md` — human-facing catalog, one row per registry rule: category, closest SonarJS/ESLint equivalent, and implementation status. Update it when adding a rule.
- `nullscan-core/tests/rules.rs` + `nullscan-core/tests/fixtures/<rule-id>/` — fixture-based regression harness. It discovers fixture directories dynamically and checks `valid.ts|tsx` (zero diagnostics for that rule) and `invalid.ts|tsx` (at least one). Other rules may fire on a fixture; only the rule named by the directory is asserted. Project-scoped rules are skipped.
- `nullscan-core/src/scope.rs` — owned per-file lexical scope graph (`ScopeGraph`): module/function/block/catch/class/loop/switch scopes, binding kinds, and references with read/write counts. No tree-sitter `Node` values are stored. Built lazily by `analyze` only when an enabled file rule has `requires_scope: true`.
- `nullscan-core/src/control_flow.rs` — small reusable statement helpers, deliberately not a CFG: `statement_terminates`, `last_statement`/`last_executable_statement`, `statements`, `body_statements`, `is_function_like`, `is_loop`, and `collect_returns` (skipping nested functions). The reachability/dataflow rules share these instead of duplicating tree walks; termination coverage is limited to return/throw/break/continue, terminating blocks, and `if`/`else`. Unit tests for the helpers live in the module.
- `nullscan-core/src/lang/typescript/rules/` — one module per per-file rule; each exposes `pub fn run(&FileContext) -> Vec<Diagnostic>`. Shared query helper in `query.rs`. Scope-dependent rules read `ctx.scopes` and return early when it is `None`.
- `nullscan-core/src/cognitive_complexity.rs` and `duplication.rs` — metric and clone-detection implementations, kept at the crate root; the TypeScript rule module only wraps them.
- `nullscan-workspace/` — everything that must behave identically in the CLI and the editor integration. `walk.rs` owns `SKIPPED_DIRS` and the recursive file collector (sorted/deduplicated files plus warnings); `config.rs` owns `nullscan.toml` loading and validation; `scan.rs` owns reading + parallel analysis + duplicate detection + the `maxDiagnostics` cap and returns a `ScanReport`; `json.rs` owns the serializable `Summary`/JSON contract (0-based UTF-16 ranges). It must stay free of `clap` and any LSP crate, and it must not depend on `nullscan-lsp`.
- `nullscan/` — thin CLI: `clap` argument parsing, calls `nullscan_workspace::scan`, then text/SARIF/JSON formatting and exit codes. No walking, config parsing or analysis orchestration lives here.
- `nullscan-lsp/` — `bin nullscan-lsp`: the stdio JSON-RPC transport and argument parsing (`main.rs`), the open-document store, custom request handlers and summary cache (`server.rs`), the workspace-scan worker job (`requests.rs`), client settings (`settings.rs`), the analyze-on-type debouncer (`debounce.rs`), outgoing notifications shared with the worker (`messages.rs`), core→LSP diagnostic mapping with UTF-16 ranges (`convert.rs`), and `file:` URI↔path conversion (`uri.rs`). It owns the LSP transport and buffer overlays; `nullscan` must never depend on it and it must not depend on `nullscan` or `clap`.
- Diagnostics are pushed on `didOpen`/`didSave` (each gated by its setting) and, when `analyzeOnType` is on, after the debounce; full text sync, `positionEncoding: utf-16`, `source: "nullscan"` and the rule id as `code`. `didChange` always refreshes the stored buffer and only queues debounced work. Every log line goes to stderr because stdout is the JSON-RPC channel.
- `nullscan-lsp/src/settings.rs` owns the client settings (`analyzeOnOpen`, `analyzeOnSave`, `analyzeOnType`, `includeDuplicates`, `maxDiagnostics`), parsed from `initializationOptions` and refreshed by `workspace/didChangeConfiguration`. Both `{ "nullscan": { ... } }` and flat payloads are accepted: nested keys win over flat ones, unknown keys are ignored, a wrong type keeps the current/default value, and an update only touches the keys it names. `maxDiagnostics` replaces the old hardcoded scan cap, and `includeDuplicates: false` adds `duplicate-code` to each scan config's `disabled_rules` without changing the workspace API.
- Analyze-on-type is debounced in `nullscan-lsp/src/debounce.rs`: `didChange` queues the URI on a dedicated thread that waits 250 ms of quiet (`DEBOUNCE_INTERVAL`; latest edit per URI wins, the interval is injectable for tests) and hands it back so the dispatch loop `select!`s on `debounce_results()` alongside client messages and job results. `Server::analyze_debounced` re-analyzes the current buffer only when the document is still open and the setting is still on, which is how `didClose` drops pending work.
- `workspace/didChangeWatchedFiles` is the only config-watch input; the client owns watcher registration and the server just reacts. A change whose file name is `nullscan.toml` republishes every open document under the reloaded config and marks the summary stale; other watched files are logged at debug and ignored. A config change never starts a workspace scan on its own.
- Two custom requests. `nullscan/analyzeWorkspace` takes an optional `{ "folders": ["file:///..."] }`, defaults to every folder from `initialize`, and returns the same `summary` object as `--format json` via `nullscan_workspace::json::summary_json`. `nullscan/summary` returns `{ "summary": <object or null>, "stale": <bool> }` from the cache. The cache is marked stale by any `didOpen`/`didChange`/`didSave`/`didClose` and fresh again only after a successful scan.
- Workspace scans never run on the dispatch loop: the worker owns a snapshot (folders, overrides, `AtomicBool`, sink clone) and answers through a `crossbeam-channel` job channel that the main loop `select!`s against client messages. Only one scan runs at a time (a second request fails with `RequestFailed`), `$/cancelRequest` sets that scan's flag, and only the dispatch thread applies the cache, publishes diagnostics and sends the `Response`.
- Each folder is scanned with its own `nullscan_workspace::config::load_in_dir` and merged with `nullscan_workspace::merge`; open buffers travel as `ScanOptions::overrides` so unsaved text wins. Published URIs are tracked, so a scan that no longer includes a file clears its diagnostics with an empty list (`version: None` for disk files).
- Config comes from each workspace folder's `nullscan.toml` via `nullscan_workspace::config::load_in_dir`; a broken file logs `window/logMessage` and falls back to defaults instead of failing the server.
- `nullscan_core::analyze()` (in `lib.rs`) parses once, builds one `FileContext { tree, source, config, scopes }`, and runs every enabled `RuleScope::File` entry from `RULES` in registry order. It builds the `ScopeGraph` only when at least one enabled file rule sets `requires_scope: true`, then passes it as `ctx.scopes`; otherwise that field is `None`. The public `analyze(source, language, config)` signature is unchanged. TS and TSX share one rule set, so queries must be built with `tree.language()` rather than a hard-coded grammar.
- Cross-file `duplicate-code` is registered in `RULES` as `RuleScope::Project` with `run: None`; `nullscan-workspace::scan` orchestrates it with `find_duplicates` because it needs all sources. Rules needing whole-project context do not belong in `analyze()`.
- The SARIF catalog (`nullscan/src/sarif.rs`) and `rules.disabled` validation (`nullscan-workspace/src/config.rs`) both read `nullscan_core::RULES`; there is no duplicate metadata list in the CLI or workspace.
- Exit codes are `0` clean, `1` findings (diagnostics or duplicates), and `2` tool error (invalid config or no existing input path). `rayon` parallel maps collect in input order, so output order stays deterministic; preserve that when touching the workspace scan pipeline.
- `--format json` is the machine contract used by editors: `schemaVersion: 1`, tool/config/summary/diagnostics/duplicates/warnings sections, and a trailing newline. Diagnostic ranges use 0-based lines and 0-based UTF-16 characters; duplicate spans keep core's 1-based line numbers. Warnings are part of the document, never stderr. Text and SARIF keep their existing shapes (SARIF adds `level`).

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
- For a new threshold, add it to `nullscan_core::Config` defaults **and** `FileThresholdsConfig` in `nullscan-workspace/src/config.rs` (kebab-case `#[serde(rename = ...)]`).
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
- Directory walks (`nullscan-workspace/src/walk.rs`) skip dot-directories and `SKIPPED_DIRS` (`node_modules`, `dist`, `build`, `.next`, `out`) to avoid scanning generated output. Explicit unsupported files and missing paths become warnings; a run with no existing input path exits `2`.
- Default config is `./nullscan.toml` relative to the process CWD, not the target path. The loader lives in `nullscan-workspace/src/config.rs`; the resolved path is reported absolute in JSON.
