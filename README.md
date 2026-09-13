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
```

Prints one diagnostic per line (`file:line:column: [rule] message`) and exits with
code 1 if anything was found, 0 if the file is clean.

## Implemented rules

| Rule | What it detects |
|---|---|
| `no-console` | Calls to `console.*` |
| `no-debugger` | `debugger;` statements |
| `no-var` | `var` instead of `let`/`const` |
| `eqeqeq` | `==`/`!=` instead of `===`/`!==` |
| `no-explicit-any` | Explicit `any` type |
| `no-empty-catch` | Empty `catch` block |

## Development

```sh
cargo test --workspace
```

See [FINDINGS.md](./FINDINGS.md) for technical gotchas discovered along the way
(tree-sitter, rule architecture, etc.).

## Roadmap

- Stateful rules (cyclomatic complexity, unused variables, code duplication)
- SARIF output for CI/editor integration
- Multi-file / glob analysis
- More languages
