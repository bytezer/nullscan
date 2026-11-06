# Nullscan for VS Code

Thin VS Code client for the `nullscan-lsp` language server. All analysis happens in the
server; this extension only starts it, shows diagnostics and exposes the workspace summary.

## Requirements

- VS Code 1.91 or newer on Linux, macOS or Windows (x64 or arm64).
- A `nullscan-lsp` binary from the same release as this extension.

The extension resolves the server in this order:

1. the `nullscan.path` setting;
2. a bundled `bin/<platform>-<arch>/nullscan-lsp` inside the extension;
3. a downloaded binary cached in the extension's global storage;
4. the first `nullscan-lsp` on `PATH`.

When the setting is empty and no bundled binary exists, the extension downloads the
matching release on first use. The download is skipped when `nullscan.path` is set.

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

## Commands

- `Nullscan: Analyze Workspace`
- `Nullscan: Show Summary`
- `Nullscan: Refresh Issues`
- `Nullscan: Open Config`
- `Nullscan: Redownload Server`
- `Nullscan: Restart Server`

`Nullscan: Redownload Server` deletes the cached binary and downloads it again; use it
after a failed download or to repair a corrupted cache entry.
