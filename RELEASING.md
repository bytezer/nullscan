# Releasing

A release is one git tag. The tag drives both binaries and the VS Code extension, so the
version lives in exactly two places and `scripts/check-version.sh` refuses to publish when
they disagree with the tag.

## Version sources

| What | Where |
|---|---|
| Rust workspace (CLI + language server) | `[workspace.package].version` in `Cargo.toml` |
| VS Code extension | `"version"` in `editors/vscode/package.json` |

Members use `version.workspace = true` and the internal path dependencies carry no
`version`, so bumping the workspace table is enough on the Rust side.

## Checklist

1. Bump both versions to the same value, for example `0.2.0` or `0.2.0-beta.1`:

   - `[workspace.package].version` in `Cargo.toml`;
   - `"version"` in `editors/vscode/package.json`.

   Every crate uses `version.workspace = true`, so the Rust side is one edit. For the
   extension, `pnpm --dir editors/vscode version 0.2.0 --no-git-tag-version` updates only
   `package.json`; editing it by hand has the same effect.

2. Make sure the tree is green:

   ```sh
   cargo test --workspace
   pnpm --dir editors/vscode typecheck && pnpm --dir editors/vscode test && pnpm --dir editors/vscode build
   ./scripts/check-version.sh v0.2.0         # must print ok before you tag
   ```

3. Commit the bump with a `chore(release): 0.2.0` style message, then tag and push:

   ```sh
   git tag v0.2.0
   git push origin v0.2.0
   ```

   The tag must be `v<version>`; `scripts/check-version.sh` strips the leading `v` and
   compares the rest with both version sources.

4. Watch the **Release** workflow (`.github/workflows/release.yml`) in the Actions tab.
   `verify` runs first; `build` (five targets), `vsix` and `release` run after it.

5. Verify the published release contains all of this:

   - five pairs of raw binaries — `nullscan-<triple>` and `nullscan-lsp-<triple>`, with an
     `.exe` suffix on Windows — for `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`,
     `x86_64-apple-darwin`, `aarch64-apple-darwin` and `x86_64-pc-windows-msvc`;
   - `checksums.txt` with one `<sha256>  <asset>` line for each of the 11 other assets
     (the manifest does not hash itself), so the release holds 12 files in total;
   - `nullscan-<version>.vsix`.

   The `release` job fails before publishing if any expected asset is missing or if the
   staging directory does not hold exactly 11 files (10 binaries plus the VSIX).

   `checksums.txt` is not cosmetic: `editors/vscode/src/download.ts` fetches it first and
   verifies the SHA-256 of the `nullscan-lsp-<triple>` it downloads against it. A missing
   entry means the extension refuses to install the server.

## Re-running a release

Nothing is published until the `release` job runs, so a failed build can be retried:

- **Same tag:** open the failed run in the Actions tab and use **Re-run failed jobs**, or
  **Re-run all jobs**. `concurrency` never cancels an in-flight release.
- **Manual trigger:** run the **Release** workflow from the Actions tab with the `tag`
  input set to an existing tag. Everything is rebuilt from that tag, so the artifacts always
  match the commit the tag points at.
- **A failed `release` job after a partial upload:** delete the draft/partial release if one
  exists, then re-run the job. `softprops/action-gh-release` uploads with `--clobber`, so a
  retry replaces assets instead of failing on duplicates.

If `verify` fails, the tag and the versions disagree. Bump the versions, commit, delete the
tag (`git tag -d v0.2.0 && git push origin :refs/tags/v0.2.0`) and tag again; never move a
tag that already produced a release.

## Installing the VSIX

Download `nullscan-<version>.vsix` from the release page and install it from a terminal:

```sh
code --install-extension nullscan-0.2.0.vsix
```

Or use **Extensions** → **…** → **Install from VSIX…**. The extension resolves or downloads
a matching `nullscan-lsp` on first use; see `editors/vscode/README.md`.

## Not covered yet

The VSIX is published only as a release asset. There is no Marketplace or Open VSX
publication in `release.yml`; adding one is a separate `vsce publish` step with a
Marketplace token.
