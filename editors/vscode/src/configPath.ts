import { isAbsolute, normalize, resolve } from "node:path";

/**
 * What `nullscan.configPath` resolved to.
 *
 * `unresolved` means the setting is a relative path but the window has no workspace folder
 * to anchor it, so the client leaves the flag out instead of guessing the extension host's
 * working directory.
 */
export type ConfigPathResolution =
  | { kind: "unset" }
  | { kind: "resolved"; path: string }
  | { kind: "unresolved"; configured: string };

/**
 * Resolves `nullscan.configPath` into the value passed to `nullscan-lsp --config`.
 *
 * Relative paths are resolved against the first workspace folder, which is the same root
 * the client uses as the server's working directory. Path resolution stays in this module
 * so it is testable without the VS Code API.
 */
export function resolveExplicitConfigPath(
  configured: string,
  workspaceFolder: string | undefined,
): ConfigPathResolution {
  const trimmed = configured.trim();
  if (trimmed.length === 0) {
    return { kind: "unset" };
  }
  if (isAbsolute(trimmed)) {
    return { kind: "resolved", path: normalize(trimmed) };
  }
  if (workspaceFolder === undefined) {
    return { kind: "unresolved", configured: trimmed };
  }
  return { kind: "resolved", path: resolve(workspaceFolder, trimmed) };
}
