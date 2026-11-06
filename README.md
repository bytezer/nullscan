# nullscan

[![CI](https://github.com/bytezer/nullscan/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/bytezer/nullscan/actions/workflows/ci.yml)

A fast, open-source static code analyzer written in Rust. Starts with TypeScript support.
Born as a lightweight alternative to SonarQube: no JVM, first-class CLI, and none of the
community edition's limitations (branch analysis, etc.).

## Current state

Working PoC: TypeScript parsing via [tree-sitter](https://tree-sitter.github.io/tree-sitter/)
and a CLI that runs a set of rules against a `.ts` file.

## Structure

```
nullscan/
├── nullscan-core/       # parsing (tree-sitter-typescript) + rules
├── nullscan-workspace/  # file walking, nullscan.toml, scan orchestration, JSON contract
├── nullscan/            # CLI (clap)
├── nullscan-lsp/        # language server (stdio JSON-RPC)
└── editors/vscode/      # VS Code client and UI
```

## Usage

```sh
cargo run -p nullscan -- file.ts
cargo run -p nullscan -- src/*.ts          # your shell expands the glob
cargo run -p nullscan -- src/              # walked recursively for .ts/.tsx
```

Directory arguments are walked recursively, skipping `node_modules` and dotdirs.
Prints one diagnostic per line (`file:line:column: [rule] message`). Exit codes are
`0` clean, `1` findings (diagnostics or duplicates), and `2` tool error (invalid
config or no existing input path).

Add `--format sarif` for [SARIF 2.1.0](https://sarifweb.azurewebsites.net/) JSON output
instead — consumed natively by GitHub Code Scanning and editor SARIF viewers, so
findings show up as inline PR annotations with no extra tooling:

```sh
cargo run -p nullscan -- src/ --format sarif > results.sarif
```

Add `--format json` for the machine-readable contract used by editors and CI. It is the
stable interface: 0-based lines and 0-based UTF-16 characters, per-severity and per-rule
summary counts, duplicate locations and warnings. See `nullscan-workspace/src/json.rs`
for the exact shape.

```sh
cargo run -p nullscan -- src/ --format json | jq .summary
```

## Editor integration

The VS Code extension in `editors/vscode` is a thin client for `nullscan-lsp`, a stdio
language server built from the same crates as the CLI: it publishes diagnostics on open and
save, offers a disable-rule quick fix, a rule hover, an issue tree and a workspace summary.
It resolves the server from `nullscan.path`, a bundled binary, a verified download or
`PATH`, honours `nullscan.toml` per workspace folder (or one file via `nullscan.configPath`),
and never runs analysis itself. See `editors/vscode/README.md` for install, settings and
limitations.

The integration suite runs a real VS Code instance against a local server build:

```sh
cargo build -p nullscan-lsp
NULLSCAN_LSP_PATH=$(pwd)/target/debug/nullscan-lsp pnpm --dir editors/vscode test:integration
```

## Install the VS Code extension

Every release ships a VSIX plus the matching language server binaries. Download the
`.vsix` from [Releases](https://github.com/bytezer/nullscan/releases) and install it from a
terminal:

```sh
code --install-extension nullscan-0.1.0.vsix
```

Or use **Extensions** → **…** → **Install from VSIX…** in VS Code. On first use the
extension resolves `nullscan-lsp` from `nullscan.path`, a bundled binary, a verified
download of the same release, or `PATH`; the download is checked against the release's
`checksums.txt`. See `editors/vscode/README.md` for settings, commands and limitations.

## Configuration

Drop a `nullscan.toml` next to where you run the CLI (or point at one with
`--config path/to/file.toml`) to disable rules, override thresholds or exclude paths.
Everything is optional; anything omitted keeps its built-in default:

```toml
[rules]
disabled = ["no-console", "no-var"]

[thresholds]
cognitive-complexity = 20        # default: 15
duplicate-code-min-tokens = 50   # default: 30
duplicate-string-threshold = 3   # default: 3
duplicate-string-min-length = 10 # default: 10
max-switch-cases = 10            # default: 10
no-small-switch-min-cases = 3    # default: 3

# Excluded from every analysis (file rules and duplicate detection).
[files]
ignore = ["tests/**", "**/*.test.ts", "src/generated/**"]

# Still analyzed for rules, but excluded from clone detection.
[duplicates]
ignore = ["tests/mocks/**", "**/__fixtures__/**"]
```

An unknown name in `disabled` prints a warning (likely a typo) but doesn't fail the
run. Disabling `duplicate-code` skips that pass entirely rather than just hiding its
output, since it's the most expensive rule to compute.

Ignore patterns are relative to the directory that contains `nullscan.toml` (or the
working directory when there is no config file). `*` and `?` stay inside one path
segment, `**` spans any number of segments, a trailing `/` covers the whole subtree, and
a pattern without `/` matches any segment, so `tests` covers both `tests/` and
`src/tests/`. `[files].ignore` removes files from the run entirely; `[duplicates].ignore`
keeps them analyzed but out of clone detection, which is what fixtures and mocks need.

## Suppressing findings

Inline comments suppress file-scoped diagnostics without touching the shared config:

```ts
// nullscan-ignore-next-line
console.log("suppresses every rule on the next line");

// nullscan-ignore-next-line no-console, eqeqeq
console.log(a == b);

eval(source); // nullscan-ignore-line no-eval

/* nullscan-ignore-next-line */
debugger;
```

Only real comments count: a marker inside a string, template literal or JSX text does
nothing. A marker without a rule list suppresses every rule; a list suppresses only the
named ones. In the editor, the `Suppress <rule> on the next line` quick fix inserts the
comment for you. Suppressions apply to per-file rules; `duplicate-code` is controlled with
`[duplicates].ignore`.

## Implemented rules

See [RULES.md](./RULES.md) for the canonical catalog: categories, closest SonarJS/ESLint
equivalents, and implementation status for every registry entry (including the
project-scoped `duplicate-code`).

| Rule | What it detects |
|---|---|
| `no-console` | Calls to `console.*` |
| `no-debugger` | `debugger;` statements |
| `no-var` | `var` instead of `let`/`const` |
| `eqeqeq` | `==`/`!=` instead of `===`/`!==` |
| `no-explicit-any` | Explicit `any` type |
| `no-empty-catch` | Empty `catch` block |
| `cognitive-complexity` | Function whose [cognitive complexity](https://www.sonarsource.com/docs/CognitiveComplexity.pdf) exceeds 15 |
| *(cross-file)* `duplicate code` | Copy-pasted blocks of 30+ tokens, across or within files |
| `no-duplicate-string` | Same string literal (10+ chars) repeated 3+ times in a file |
| `no-identical-conditions` | `if`/`else if` whose condition repeats an earlier one in the same chain |
| `no-redundant-boolean` | `x === true`/`x == false` comparisons, and `!!x` double negation |
| `prefer-immediate-return` | `const x = expr; return x;` instead of `return expr;` |
| `no-collapsible-if` | Nested `if` with no `else` on either level, mergeable with `&&` |
| `no-identical-expressions` | Same expression on both sides of a binary operator |
| `no-duplicated-branches` | Two branches of a conditional with the same implementation |
| `no-all-duplicated-branches` | Every branch of a conditional with the same implementation |
| `no-useless-catch` | `catch` that only rethrows the caught error |
| `no-redundant-jump` | Bare `return;` at the end of a function, bare `continue;` at the end of a loop |
| `prefer-single-boolean-return` | `if`/`else` returning only `true` and `false` |
| `no-nested-switch` | `switch` nested inside another `switch` (without a function boundary) |
| `no-nested-template-literals` | Template literal nested in another template literal |
| `no-small-switch` | `switch` with fewer than 3 `case` clauses (configurable) |
| `max-switch-cases` | `switch` with more than 10 `case` clauses (configurable) |
| `no-same-line-conditional` | `if` starting on the same line as the previous statement |
| `non-existent-operator` | `=+`, `=-`, or `=!` typos |
| `no-self-assign` | Assignment of a value to itself |
| `no-constant-condition` | Constant `if`/ternary condition |
| `use-isnan` | Comparison with `NaN` instead of `Number.isNaN` |
| `no-array-constructor` | Ambiguous `Array(...)` / `new Array(...)` calls |
| `no-new-wrappers` | `new String(...)`, `new Number(...)`, `new Boolean(...)` |
| `no-duplicate-imports` | Multiple imports from the same module |
| `no-unused-vars` | Unused imports and unused local variables |
| `no-shadow` | A binding shadows a name from an enclosing scope |
| `no-use-before-define` | A `let`/`const`/`class` use before its same-scope declaration |
| `prefer-const` | `let` declarator that is never reassigned |
| `no-redeclare` | A second binding with the same name in one scope |
| `no-unreachable` | Statement that can never execute after a terminating statement |
| `no-fallthrough` | Non-empty `switch` case that can fall into the next case |
| `consistent-return` | Function that returns a value on some paths but not others |
| `no-one-iteration-loop` | Loop that unconditionally exits on its first iteration |
| `no-unused-collection` | Local collection that is only mutated, never read |
| `no-element-overwrite` | Adjacent assignment overwrites the same target without a read |
| `no-ignored-return` | Discarded return value of a known pure function |
| `no-gratuitous-expressions` | `a && a`, `a || !a`, etc. in a condition |
| `no-eval` | Direct `eval(...)` calls, including `window.eval` and friends |
| `no-implied-eval` | `setTimeout`/`setInterval`/`setImmediate`/`execScript` called with a string |
| `no-new-func` | `new Function(...)` and `Function(...)` |
| `no-script-url` | String values starting with `javascript:` |
| `hardcoded-secrets` | Secret-like names assigned a hard-coded string (heuristic) |

## Performance

Per-file analysis and duplication tokenization both run in parallel via `rayon`.
On a 2,161-file real-world monorepo (20 cores): 43s → 8s.

## Development

```sh
cargo test --workspace
```

Rule behavior is covered twice: unit tests in `nullscan-core/src/lang/typescript.rs`
assert exact diagnostics, and the fixture harness in `nullscan-core/tests/rules.rs`
discovers `nullscan-core/tests/fixtures/<rule-id>/{valid,invalid}.ts|tsx`. Every
file-scoped rule needs at least one `valid` fixture (zero diagnostics for that rule)
and one `invalid` fixture (at least one diagnostic); the harness fails if a pair is
missing, so a new rule cannot land without regression coverage.

See [FINDINGS.md](./FINDINGS.md) for technical gotchas discovered along the way
(tree-sitter, rule architecture, etc.).

## Releasing

CI runs on every push to `main` and every pull request (`.github/workflows/ci.yml`).
Releases are tag-driven: bump `[workspace.package].version` and
`editors/vscode/package.json` together, push a `v<version>` tag, and
`.github/workflows/release.yml` builds the five binary pairs, the VSIX and `checksums.txt`
and publishes them as a GitHub Release. The full checklist, including how to re-run a failed
job, is in [RELEASING.md](./RELEASING.md); `scripts/check-version.sh <tag>` verifies the
version sources locally.

## Achievements

What's shipped, and what's next.

**Core**
- [x] TypeScript parsing via tree-sitter
- [x] CLI with per-file diagnostics and exit codes for CI
- [x] Multi-file / directory input (glob via shell expansion)
- [x] Parallelized analysis (`rayon`)
- [x] Reusable per-file lexical scope graph (bindings, references, read/write counts)
- [x] SARIF output for CI/editor integration
- [x] VS Code extension (inline diagnostics, issue trees, workspace summary)
- [x] Configurable rule thresholds and disabling (`nullscan.toml` / `--config`)
- [ ] More languages beyond TypeScript

**Rules**
- [x] `no-console`
- [x] `no-debugger`
- [x] `no-var`
- [x] `eqeqeq`
- [x] `no-explicit-any`
- [x] `no-empty-catch`
- [x] `cognitive-complexity` (per function)
- [x] cross-file `duplicate code` (token-window based)
- [x] `no-duplicate-string` — same string literal repeated 3+ times
- [x] `no-identical-conditions` — repeated `if`/`else if` condition
- [x] `no-redundant-boolean` — `x === true`, `!!x`
- [x] `prefer-immediate-return` — declare-then-return in one step
- [x] `no-collapsible-if` — nested `if`s with no `else`, mergeable with `&&`
- [x] `no-identical-expressions` — identical operands around a binary operator
- [x] `no-duplicated-branches` — two branches with the same implementation
- [x] `no-all-duplicated-branches` — all branches with the same implementation
- [x] `no-useless-catch` — `catch` that only rethrows
- [x] `no-redundant-jump` — redundant `return;` / `continue;`
- [x] `prefer-single-boolean-return` — `if`/`else` returning `true`/`false`
- [x] `no-nested-switch` — nested `switch` statements
- [x] `no-nested-template-literals` — nested template literals
- [x] `no-small-switch` — too few `case` clauses
- [x] `max-switch-cases` — too many `case` clauses
- [x] `no-same-line-conditional` — conditional on the previous statement's line
- [x] `non-existent-operator` — `=+`, `=-`, `=!` typos
- [x] `no-self-assign` — self-assignment
- [x] `no-constant-condition` — constant `if`/ternary conditions
- [x] `use-isnan` — `NaN` comparisons
- [x] `no-array-constructor` — ambiguous `Array` constructors
- [x] `no-new-wrappers` — primitive wrapper objects
- [x] `no-duplicate-imports` — repeated imports from one module
- [x] `no-unused-vars` — unused imports and local variables
- [x] `no-shadow` — bindings shadowing an outer scope
- [x] `no-use-before-define` — same-scope use before declaration
- [x] `prefer-const` — `let` variables that are never reassigned
- [x] `no-redeclare` — duplicate bindings in one scope
- [x] `no-unreachable` — statements after a terminating jump
- [x] `no-fallthrough` — `switch` cases that fall into the next case
- [x] `consistent-return` — mixed valued and bare/falling-off returns
- [x] `no-one-iteration-loop` — loops that exit on their first iteration
- [x] `no-unused-collection` — collections that are only mutated
- [x] `no-element-overwrite` — adjacent assignments to the same target
- [x] `no-ignored-return` — discarded pure return values
- [x] `no-gratuitous-expressions` — constant `&&`/`||` conditions
- [x] `no-eval` — direct `eval` calls
- [x] `no-implied-eval` — timers called with a string
- [x] `no-new-func` — `Function` constructor
- [x] `no-script-url` — `javascript:` URLs
- [x] `hardcoded-secrets` — heuristic hard-coded secret detection
- [ ] `no-identical-functions` — whole duplicated functions
- [ ] `max-params`
- [ ] `cognitive-complexity` at class/file level
