# Nullscan for VS Code

Thin VS Code client for the `nullscan-lsp` language server. All analysis happens in the
server; this extension only starts it, shows diagnostics and exposes the workspace summary.

## Requirements

- A `nullscan-lsp` binary built from the same release as this extension.
- Resolution order: the `nullscan.path` setting, `bin/<platform>-<arch>/nullscan-lsp` inside
  the extension, then the first `nullscan-lsp` found on `PATH`.

## Development

```sh
pnpm install
pnpm typecheck
pnpm test
pnpm build
```

Set `nullscan.path` to a local build (for example
`target/release/nullscan-lsp`) and run the extension with F5.

## Commands

- `Nullscan: Analyze Workspace`
- `Nullscan: Show Summary`
- `Nullscan: Open Config`
- `Nullscan: Restart Server`

The summary webview, the diagnostics tree and the automatic binary download are not part
of this scaffold yet.
