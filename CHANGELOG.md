# Changelog

All notable changes to this project are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the
project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html) with
pre-release versions such as `0.x.0-beta.1` for testers.

## [Unreleased]

## [0.1.0-beta.5] - 2026-11-12

### Added

- **`no-floating-promises`** (typescript-eslint `no-floating-promises`) flags a promise
  that is neither awaited, ended with `.catch` or a two-argument `.then`, nor marked with
  `void`. Beyond Promise statics, `fetch`, async IIFEs and same-file async functions, it
  follows explicit type annotations (parameters, destructuring, `const x: T`, member
  access) through same-file aliases and interfaces and through relative imports, so
  `persistence?.onSave(next)` is reported when `onSave` returns `void | Promise<void>` in
  another file. Type inference, generics, bare specifiers and tsconfig `paths` are not
  followed.
- **Four rules** — `no-literal-call` (SonarJS S6958) flags a literal called as a function,
  which always throws a `TypeError` at runtime; `no-useless-increment` (S2123) flags an
  increment or decrement whose result is discarded; `no-generator-without-yield` (S3531)
  flags a generator that never yields, so it finishes on the first `next()` call; and
  `no-unthrown-error` (S3984) flags an `Error` that is created and never thrown.
- **Documentation reference** — `docs/configuration.md` documents every `nullscan.toml`
  key (type, default, scope and effect), config-file resolution, glob syntax, validation
  and recipes; `docs/cli.md` covers install, options, exit codes and output formats; and
  `docs/json.md` specifies the `--format json` contract, including the 0-based UTF-16
  diagnostic ranges versus the 1-based duplicate line spans.
- `RULES.md` now has a **Configurable rules** table linking each tunable rule to its key,
  default and reference section.

### Changed

- Releases build and publish only the two Linux targets
  (`x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`) during the beta, cutting macOS
  and Windows build time; macOS and Windows users can still build from source and point
  `nullscan.path` at the result.
- Corrected the VS Code README: inline suppressions are implemented, and the extension
  documentation now states the Linux-only prebuilt server availability.

## [0.1.0-beta.4] - 2026-11-06

Real-repository round: more server-side signal, fewer false positives, and a human
duplication measure.

### Added

- **Three rules** — `no-alphabetical-sort` (SonarJS S2871) flags `sort()`/`toSorted()`
  without a compare function; `no-nested-ternary` (S3358) flags ternaries nested in
  another ternary; `no-array-index-key` (S6479) flags JSX keys bound to the index
  parameter of a `map`/`flatMap` callback, resolved through the scope graph so shadowed
  names are not reported.
- **Per-rule ignored strings** — `[rules.no-duplicate-string] ignore = ["transparent", "currentColor"]` skips ubiquitous CSS values without disabling the rule.
- **Duplication density** — the JSON summary now carries `duplication.density` (percentage
  of duplicated lines of code), `duplicatedLines`, `totalLines`, `duplicatedBlocks`,
  `duplicatedFiles` and `largestBlockLines`. The VS Code summary panel renders it with a
  bar, and per-block messages show lines plus tokens.

### Fixed

- `no-use-before-define` no longer flags type-position references such as
  `interface I { key: typeof CARD_KEYS }` where `CARD_KEYS` is declared later; real
  value uses, including `class A extends B`, still report.

### Changed

- Duplicate diagnostics use the same line format as every other diagnostic and include
  the block size in lines as well as tokens.

## [0.1.0-beta.3] - 2026-11-06

Testing round focused on signal quality and on making the rules understandable
without leaving the editor.

### Added

- **Rule catalog** — every rule now has a human `title` and a one-sentence `why`,
  exposed to the extension through the `nullscan/rules` request. The **By Rule** tree
  shows the title with the rule id as description, the summary panel shows both, and a
  new **Nullscan: Show Rule Documentation** command opens `RULES.md#<rule-id>` (from a
  rule node or from a palette QuickPick).
- **Richer hover and docs links** — the hover shows the title, severity, rationale and
  the concrete message, and every diagnostic carries `codeDescription.href` pointing at
  its `RULES.md` anchor. Each catalog row has a stable `<a id="<rule-id>"></a>` anchor,
  guarded by a test so docs links cannot rot.
- **Logic-only clone detection** — `duplicate-code` ignores presentational markup by
  default: JSX tags, attributes, strings, `jsx_text`, static JSX and `<svg>` subtrees do
  not count, while logic inside `{...}` does. This stops different SVG icons that share a
  header from being reported as duplicates. `[duplicates] compare-markup = true` restores
  the previous behavior.

### Changed

- **Actionable diagnostic messages** — 34 message sites now say what to do, not only what
  happened (for example `avoid 'any'; use a specific type or 'unknown'`).

## [0.1.0-beta.2] - 2026-11-06

Tester feedback round: finer control over what is analyzed, without editing rule code.

### Added

- **Path ignores in `nullscan.toml`** — `[files].ignore` removes files from every pass
  (rules and duplicates) and `[duplicates].ignore` keeps them analyzed but out of clone
  detection, which is what fixtures and mocks need. Patterns support `*`, `?`, `**`, a
  trailing `/` for a subtree, segment matching without a slash, and absolute paths;
  they are relative to the config file's directory.
- **Inline suppressions** — `// nullscan-ignore-next-line [rules]`,
  `// nullscan-ignore-line [rules]` and their `/* */` forms. Only real comments count;
  no rule list suppresses every rule.
- **`Suppress <rule> on the next line` quick fix** in the VS Code extension, next to the
  existing disable-rule action.
- **`nullscan --version`** for the CLI.

### Changed

- Files excluded by `[files].ignore` publish no diagnostics when opened in the editor.

## [0.1.0] - 2026-11-06

First tagged release: the TypeScript analyzer ships as a CLI and as a VS Code extension
backed by the same engine.

### Added

- **CLI (`nullscan`)** — analyzes `.ts`, `.mts`, `.cts` and `.tsx` files and directories,
  skipping `node_modules`, dot-directories and generated output (`dist`, `build`, `.next`,
  `out`). Output formats: text (`file:line:column: [rule] message`), SARIF 2.1.0 for code
  scanning, and a stable JSON contract (`schemaVersion: 1`, 0-based UTF-16 ranges) used by
  editors and CI. Exit codes are `0` clean, `1` findings, `2` tool error.
- **Rule set** — 49 rules across bugs, code smells, metrics, duplication and security,
  including per-function cognitive complexity, cross-file clone detection, a reusable
  lexical scope graph (unused variables, shadowing, `prefer-const`, redeclaration) and the
  security rules `no-eval`, `no-implied-eval`, `no-new-func`, `no-script-url` and
  `hardcoded-secrets`. See `RULES.md` for the full catalog and upstream equivalents.
- **Configuration** — `nullscan.toml` (or `--config <path>`) disables rules and overrides
  thresholds; unknown rule names produce a warning instead of failing the run.
- **Language server (`nullscan-lsp`)** — stdio JSON-RPC server with push diagnostics on
  open and save, full document sync, UTF-16 positions, rule hover, a disable-rule quick fix
  that edits `nullscan.toml` in place, and `duplicate-code` diagnostics linked across files.
  Custom requests `nullscan/analyzeWorkspace` (cancelable, one scan at a time) and
  `nullscan/summary` expose the workspace summary, with a per-folder config in multi-root
  workspaces and an optional `--config <path>` override.
- **VS Code extension** — diagnostics, severity status bar, **By Rule** / **By File** issue
  trees, a workspace summary panel, an output channel and commands for analyzing a
  workspace, showing the summary, opening the config, restarting the server and
  redownloading it. Settings cover the server path, config path, analyze-on-open/save/type,
  duplicates, the diagnostic cap and the status bar. The extension is a thin client: it
  never analyzes code and never shells out to the CLI.
- **Binary distribution** — releases publish raw `nullscan-<triple>` and
  `nullscan-lsp-<triple>` binaries for `x86_64-unknown-linux-gnu`,
  `aarch64-unknown-linux-gnu`, `x86_64-apple-darwin`, `aarch64-apple-darwin` and
  `x86_64-pc-windows-msvc` (`.exe`), plus `checksums.txt` and a `.vsix`. When
  `nullscan.path` is unset and no bundled binary exists, the extension downloads the
  matching server into its global storage, verifies the SHA-256 from `checksums.txt` (proxy
  aware), caches it per version and checks `--version` before using it.
- **CI and releases** — `ci.yml` runs the Rust test suite, `cargo tree` coupling guards,
  extension typecheck/unit tests/build and the real VS Code integration test under Xvfb;
  `release.yml` verifies the tag against both version sources and publishes the assets.

### Notes

- JavaScript is not analyzed yet; only TypeScript inputs are supported.
- Inline suppressions (`// nullscan-ignore`), pull diagnostics, autofix and languages other
  than TypeScript are planned fast-follows, not part of `0.1.0`.

[0.1.0]: https://github.com/bytezer/nullscan/releases/tag/v0.1.0
