# Command line

```
nullscan [OPTIONS] <PATH>...
```

`<PATH>` is one or more files or directories. Globs are expanded by your shell, not by
nullscan.

## Install

Releases publish raw binaries plus a `checksums.txt` manifest. Download the asset for your
machine from [Releases](https://github.com/bytezer/nullscan/releases):

| Asset | Platform |
|---|---|
| `nullscan-x86_64-unknown-linux-gnu` | Linux x64 |
| `nullscan-aarch64-unknown-linux-gnu` | Linux arm64 |

```sh
VERSION=0.1.0-beta.4
TRIPLE=x86_64-unknown-linux-gnu

curl -LO "https://github.com/bytezer/nullscan/releases/download/v$VERSION/nullscan-$TRIPLE"
curl -LO "https://github.com/bytezer/nullscan/releases/download/v$VERSION/checksums.txt"
sha256sum --check --ignore-missing checksums.txt
chmod +x "nullscan-$TRIPLE"
sudo mv "nullscan-$TRIPLE" /usr/local/bin/nullscan
```

There are no archives: each asset is the executable itself. The `nullscan-lsp-<triple>`
language server is published next to it for the editor integration.

> During the current beta, releases only build binaries for the two Linux targets above.
> macOS and Windows users must build from source (`cargo build --release -p nullscan`) or
> point the editor at that local build via `nullscan.path`.

## Usage

```sh
nullscan file.ts
nullscan src/*.ts             # your shell expands the glob
nullscan src/                 # walked recursively for .ts/.tsx
nullscan src/ --format sarif > results.sarif
nullscan src/ --format json | jq .summary
nullscan src/ --config configs/ci.toml
```

## Options

| Option | Default | What it does |
|---|---|---|
| `--format <text\|sarif\|json>` | `text` | Output format. See [Output formats](#output-formats). |
| `--config <path>` | `./nullscan.toml` if present | Explicit config file. Missing or invalid file exits `2`. |
| `--max-diagnostics <n>` | unlimited | Keep at most `n` per-file diagnostics, in file order. |
| `--version` | — | Print `nullscan <version>` and exit `0`. |
| `--help` | — | Print usage. |

`--config` also accepts the `--config=<path>` form. Configuration keys are documented in
[configuration.md](./configuration.md).

### `--max-diagnostics`

The cap is applied **after** analysis, in deterministic file order: the first `n`
diagnostics survive, the rest are dropped. Cross-file `duplicate-code` findings are not
capped. The JSON document reports `summary.truncated: true` when anything was dropped, and
a truncated run exits `1` (there were findings) even if the surviving list is short.

This bounds output in CI without changing how files are analyzed. To skip work instead,
disable rules in `nullscan.toml`.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Clean: no diagnostics, no duplicates, no truncation. |
| `1` | Findings: at least one diagnostic, at least one duplicate, or `--max-diagnostics` dropped anything. |
| `2` | Tool error: invalid config, or no input path exists. |

Warnings (unreadable files, unsupported inputs, unknown disabled rule ids) are non-fatal
and do not change the exit code.

## Output formats

### `text` (default)

One line per finding, 1-based line and byte column, followed by the rule id in brackets:

```
src/app.ts:12:3: [no-console] remove this console call or use the logger
src/a.ts:10:1: [duplicate-code] duplicated block of 42 lines (312 tokens), also found at src/b.ts:40-81
```

Warnings go to stderr so stdout stays parseable.

### `sarif`

[SARIF 2.1.0](https://sarifweb.azurewebsites.net/) JSON, consumed natively by GitHub Code
Scanning and SARIF viewers. Diagnostics and `duplicate-code` findings both become results,
and duplicates carry the other span as a related location. Example upload step:

```yaml
- run: nullscan src/ --format sarif > nullscan.sarif
- uses: github/codeql-action/upload-sarif@v3
  with:
    sarif_file: nullscan.sarif
```

Warnings go to stderr, outside the SARIF document.

### `json`

The stable machine contract used by editors and CI: 0-based UTF-16 diagnostic ranges, a
summary with per-severity/per-rule counts and the duplication metric, duplicate locations
and warnings inside the document. The full shape is documented in [json.md](./json.md).

## What gets scanned

- Extensions: `.ts`, `.mts`, `.cts`, `.tsx`. JavaScript is not analyzed.
- Directories are walked recursively.
- Skipped directories: dot-directories (any name starting with `.`) and `node_modules`,
  `dist`, `build`, `.next`, `out`.
- Results are sorted and deduplicated, so output order is deterministic.
- An explicit unsupported file, an unreadable directory or a missing path becomes a
  warning and the rest of the run continues. If **none** of the input paths exist,
  nullscan exits `2`.
- `[files].ignore` removes paths from the walk; `[duplicates].ignore` only keeps them out
  of clone detection. See [configuration.md](./configuration.md).
