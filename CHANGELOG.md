# Changelog

All notable changes to this project are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the
project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html) with
pre-release versions such as `0.x.0-beta.1` for testers.

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
