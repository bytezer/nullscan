# nullscan

A fast, open-source static code analyzer written in Rust. Starts with TypeScript support.
Born as a lightweight alternative to SonarQube: no JVM, first-class CLI, and none of the
community edition's limitations (branch analysis, etc.).

## Current state

Working PoC: TypeScript parsing via [tree-sitter](https://tree-sitter.github.io/tree-sitter/)
and a CLI that runs a set of rules against a `.ts` file.

## Structure

```
nullscan/
├── nullscan-core/   # parsing (tree-sitter-typescript) + rules
└── nullscan-cli/    # CLI (clap)
```

## Usage

```sh
cargo run -p nullscan-cli -- file.ts
cargo run -p nullscan-cli -- src/*.ts          # your shell expands the glob
cargo run -p nullscan-cli -- src/                # walked recursively for .ts/.tsx
```

Directory arguments are walked recursively, skipping `node_modules` and dotdirs.
Prints one diagnostic per line (`file:line:column: [rule] message`) and exits with
code 1 if anything was found, 0 if every file is clean.

Add `--format sarif` for [SARIF 2.1.0](https://sarifweb.azurewebsites.net/) JSON output
instead — consumed natively by GitHub Code Scanning and editor SARIF viewers, so
findings show up as inline PR annotations with no extra tooling:

```sh
cargo run -p nullscan-cli -- src/ --format sarif > results.sarif
```

## Implemented rules

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

## Performance

Per-file analysis and duplication tokenization both run in parallel via `rayon`.
On a 2,161-file real-world monorepo (20 cores): 43s → 8s.

## Development

```sh
cargo test --workspace
```

See [FINDINGS.md](./FINDINGS.md) for technical gotchas discovered along the way
(tree-sitter, rule architecture, etc.).

## Achievements

What's shipped, and what's next.

**Core**
- [x] TypeScript parsing via tree-sitter
- [x] CLI with per-file diagnostics and exit codes for CI
- [x] Multi-file / directory input (glob via shell expansion)
- [x] Parallelized analysis (`rayon`)
- [x] SARIF output for CI/editor integration
- [ ] VS Code extension (surface diagnostics inline as you type)
- [ ] Configurable rule thresholds (CLI flags / config file)
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
- [ ] `no-unused-vars` / `no-unused-imports`
- [ ] `no-identical-functions` — whole duplicated functions
- [ ] `max-params`
- [ ] `cognitive-complexity` at class/file level
