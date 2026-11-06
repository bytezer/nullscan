# Nullscan for VS Code

Thin VS Code client for the `nullscan-lsp` language server. All analysis happens in the
server; this extension only starts it, shows diagnostics and exposes the workspace summary.

## Install

Grab the `.vsix` from the release page and install it from the command line:

```sh
code --install-extension nullscan-0.1.0.vsix
```

Or use **Extensions** → **…** → **Install from VSIX…** in the editor.

The extension needs VS Code 1.91 or newer on Linux, macOS or Windows (x64 or arm64), and a
`nullscan-lsp` binary from the same release. It resolves the server in this order:

1. the `nullscan.path` setting;
2. a bundled `bin/<platform>-<arch>/nullscan-lsp` inside the extension;
3. a downloaded binary cached in the extension's global storage;
4. the first `nullscan-lsp` on `PATH`.

When the setting is empty and no bundled binary exists, the extension downloads the
matching release on first use. The download is skipped when `nullscan.path` is set.

## Settings

| Setting | Default | What it does |
| --- | --- | --- |
| `nullscan.path` | `""` | Path to `nullscan-lsp`. Empty means bundled binary, then download, then `PATH`. |
| `nullscan.configPath` | `""` | One `nullscan.toml` for every file and every workspace scan. Relative paths use the first folder. |
| `nullscan.analyzeOnOpen` | `true` | Analyze a document when it is opened. |
| `nullscan.analyzeOnSave` | `true` | Analyze a document when it is saved. |
| `nullscan.analyzeOnType` | `false` | Analyze shortly after the buffer changes (250 ms of quiet). |
| `nullscan.includeDuplicates` | `true` | Include `duplicate-code` findings in workspace scans. |
| `nullscan.maxDiagnostics` | `5000` | Cap on diagnostics a workspace scan may publish; the summary reports truncation. |
| `nullscan.showStatusBar` | `true` | Show the Nullscan status bar item. |

Without `nullscan.configPath` the server reads a `nullscan.toml` from each workspace folder,
so a multi-root workspace can use a different rule set per folder. With the setting, the
same file applies everywhere and the extension passes `--config <path>` to the server; a
missing file logs a warning and the server keeps running with the default rules.

## Commands

- `Nullscan: Analyze Workspace` — scan every workspace folder and publish duplicates.
- `Nullscan: Show Summary` — open the summary panel.
- `Nullscan: Refresh Issues` — rebuild the issue trees from the current diagnostics.
- `Nullscan: Open Config` — open (or create) the workspace folder's `nullscan.toml`.
- `Nullscan: Redownload Server` — delete the cached binary and download it again.
- `Nullscan: Restart Server` — restart the language server, for example after changing `nullscan.path`.

## Views

The activity bar container **Nullscan** hosts two trees, both built from the diagnostics
VS Code already holds (filtered to `source: "nullscan"`), not from a separate server call:

- **By Rule** groups issues by rule id, sorted by count and using the group's worst severity
  as its icon.
- **By File** groups issues by workspace-relative path, most issues first.

Clicking an issue opens the document and reveals its range. The trees refresh automatically
after diagnostics changes and after **Nullscan: Analyze Workspace**; the title-bar button
runs **Nullscan: Refresh Issues**. When there are no issues, the views offer a link to
**Analyze Workspace**.

## Summary

**Nullscan: Show Summary** opens (or reveals) a single *Nullscan summary* panel with severity
totals, total issues, files with issues, the top 10 rules, the top 10 hot files, the last
scan's file count, duration, duplicates and truncation flag, a *stale* badge when the
workspace changed after the scan, and the resolved `nullscan.toml` path. Its **Rescan** and
**Open config** buttons run the matching commands. The panel is plain HTML/CSS with a strict
nonce-based Content-Security-Policy, no framework and no external assets.

## Automatic download

Releases publish raw binaries (`nullscan-lsp-<target-triple>`, plus `.exe` on Windows) and
a `checksums.txt` manifest next to them. There are no archives.

| Target triple               | Platform            |
| --------------------------- | ------------------- |
| `x86_64-unknown-linux-gnu`  | Linux x64           |
| `aarch64-unknown-linux-gnu` | Linux arm64         |
| `x86_64-apple-darwin`       | macOS Intel         |
| `aarch64-apple-darwin`      | macOS Apple silicon |
| `x86_64-pc-windows-msvc`    | Windows x64         |

The asset is fetched from
`https://github.com/bytezer/nullscan/releases/download/v<version>/<asset>`. The extension
downloads `checksums.txt` first, streams the binary to a temporary file while computing its
SHA-256, and only then moves it into place with mode `0755`. GitHub redirects to its CDN
are followed manually, and the `http.proxy` and `http.proxyStrictSSL` settings are
honoured. After the download the binary must answer `--version` with the extension version
(pre-release suffixes are ignored); otherwise it is deleted and the download fails.

Downloaded servers are cached per extension version under
`<globalStorage>/server/<version>/<asset>`. Typical locations:

- Linux: `~/.config/Code/User/globalStorage/bytezer.nullscan/server/`
- macOS: `~/Library/Application Support/Code/User/globalStorage/bytezer.nullscan/server/`
- Windows: `%APPDATA%\Code\User\globalStorage\bytezer.nullscan\server\`

A cached binary is reused only when its SHA-256 matches the release's `checksums.txt`.
Network and checksum failures show an actionable error with **Open Settings**, **Show
Output** and **Retry**; every step is logged in the **Nullscan** output channel.

## Known limitations

- Duplicate findings come from a workspace scan, not from the open buffer: run
  **Nullscan: Analyze Workspace** to see `duplicate-code`, and re-run it after large edits.
  Editing a file drops its duplicate diagnostics until the next scan.
- There are no inline suppressions yet (`// nullscan-ignore` is not implemented); use
  `nullscan.toml`, `nullscan.configPath` or the *Disable rule* quick fix.
- No telemetry: nothing is collected or sent. The only network access is the optional
  server download.
- The download follows the `http.proxy` / `http.proxyStrictSSL` VS Code settings. A proxy
  that needs authentication or a custom CA may need `nullscan.path` pointing at a local
  build instead.
- Only TypeScript and TSX are analyzed. JavaScript files are not part of the document
  selector and are skipped by workspace scans.

## Troubleshooting

- **No diagnostics** — open the **Nullscan** output channel (View → Output), check that the
  server started, and look at the `nullscan-lsp` version line. A mismatched server and
  extension version is reported as a warning.
- **Server not found / download failed** — set `nullscan.path` to a local
  `nullscan-lsp` binary, or run **Nullscan: Redownload Server** to delete the cached copy
  and download it again. **Nullscan: Restart Server** applies a new path.
- **Wrong rules are applied** — check `nullscan.configPath` first: when set it overrides
  every folder's `nullscan.toml`. Relative paths resolve against the first workspace
  folder, and a path that does not exist logs a warning and falls back to the defaults.
  **Nullscan: Open Config** edits the folder config used when the setting is empty.

## Development

```sh
pnpm install
pnpm typecheck
pnpm test
pnpm build
```

Set `nullscan.path` to a local build (for example
`target/release/nullscan-lsp`) and run the extension with F5. To exercise the downloader
against a local release mirror, set `NULLSCAN_DOWNLOAD_BASE_URL` (tests only).

The integration suite starts a real VS Code instance through `@vscode/test-electron`. Build
the server first, then point the runner at it:

```sh
cargo build -p nullscan-lsp
NULLSCAN_LSP_PATH=$(pwd)/target/debug/nullscan-lsp pnpm --dir editors/vscode test:integration
```

The `pre-test` step bundles the extension; the test itself sets `nullscan.path` from
`NULLSCAN_LSP_PATH`, opens `test/fixtures/workspace/src/app.ts` and waits up to 10 s for a
`no-console` diagnostic published with `source: "nullscan"`.

## Packaging

```sh
pnpm --dir editors/vscode package --no-dependencies --out /tmp/nullscan.vsix
```

`.vscodeignore` keeps `src/`, `test/`, `out/`, `node_modules/` and build configs out of the
archive; esbuild bundles every runtime dependency into `dist/extension.js`.
