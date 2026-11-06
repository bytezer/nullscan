import { accessSync, constants } from "node:fs";
import { posix, win32 } from "node:path";

/** Where a resolved server binary came from. */
export type BinarySource = "setting" | "bundled" | "download" | "path";

/**
 * `path`/`source` describe a usable binary; `error`/`candidates` describe every location
 * that was checked, so the caller can show an actionable message.
 */
export type ResolveBinaryResult =
  | { path: string; source: BinarySource }
  | { error: string; candidates: string[] };

export interface ResolveBinaryOptions {
  /** Value of the `nullscan.path` setting; empty when unset. */
  configuredPath: string;
  /** Directory of the installed extension. */
  extensionPath: string;
  /** `process.platform`, injectable for tests. */
  platform: string;
  /** `process.arch`, injectable for tests. */
  arch: string;
  /** `process.env.PATH`, injectable for tests. */
  envPath?: string | undefined;
  /** Cached download for this extension version; takes precedence over `PATH`. */
  downloadedPath?: string | undefined;
  /** Set to `false` to skip the `PATH` lookup (used before downloading). */
  includePath?: boolean;
  /** Existence check, injectable for tests. */
  isExecutable?: (candidate: string) => boolean;
}

interface Candidate {
  path: string;
  source: BinarySource;
}

/** Release target triple per `<platform>-<arch>` pair; shared with the downloader. */
const TRIPLES: Readonly<Record<string, string>> = {
  "linux-x64": "x86_64-unknown-linux-gnu",
  "linux-arm64": "aarch64-unknown-linux-gnu",
  "darwin-x64": "x86_64-apple-darwin",
  "darwin-arm64": "aarch64-apple-darwin",
  "win32-x64": "x86_64-pc-windows-msvc",
};

/** e.g. `linux-x64`; `undefined` for a combination without a prebuilt server. */
export function serverTarget(platform: string, arch: string): string | undefined {
  return serverTriple(platform, arch) === undefined ? undefined : `${platform}-${arch}`;
}

/** e.g. `x86_64-unknown-linux-gnu`; `undefined` for an unsupported combination. */
export function serverTriple(platform: string, arch: string): string | undefined {
  return TRIPLES[`${platform}-${arch}`];
}

/** Resolves the server in `nullscan.path` → bundled → downloaded cache → `PATH` order. */
export function resolveServerBinary(options: ResolveBinaryOptions): ResolveBinaryResult {
  const isExecutable = options.isExecutable ?? isExecutableFile;
  const configuredPath = options.configuredPath.trim();

  if (configuredPath.length > 0 && isExecutable(configuredPath)) {
    return { path: configuredPath, source: "setting" };
  }

  const target = serverTarget(options.platform, options.arch);
  if (target === undefined) {
    return {
      error: unsupportedTargetMessage(options.platform, options.arch),
      candidates: configuredPath.length > 0 ? [configuredPath] : [],
    };
  }

  const candidates = candidatePaths(options, target);
  const found = candidates.find((candidate) => isExecutable(candidate.path));
  if (found !== undefined) {
    return { path: found.path, source: found.source };
  }

  return { error: missingBinaryMessage(candidates), candidates: candidates.map((c) => c.path) };
}

/** Parses `nullscan-lsp 0.1.0` (and prerelease/build variants) from `--version` output. */
export function parseServerVersion(stdout: string): string | undefined {
  return /nullscan-lsp\s+(\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?)/.exec(
    stdout,
  )?.[1];
}

/**
 * Compares the semver core, so `0.1.0-beta.2` matches `0.1.0` in either direction.
 * Pre-release tags and build metadata are ignored on purpose: the binary and the
 * extension ship from the same release tag.
 */
export function serverVersionMatches(expected: string, actual: string): boolean {
  return semverCore(expected) === semverCore(actual);
}

function semverCore(version: string): string {
  return version.trim().replace(/^v/, "").split(/[-+]/, 1)[0] ?? version.trim();
}

/** Default existence check: the candidate must be present and executable. */
export function isExecutableFile(candidate: string): boolean {
  try {
    accessSync(candidate, constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

function candidatePaths(options: ResolveBinaryOptions, target: string): Candidate[] {
  const pathApi = options.platform === "win32" ? win32 : posix;
  const binaryName = options.platform === "win32" ? "nullscan-lsp.exe" : "nullscan-lsp";
  const candidates: Candidate[] = [];
  const configuredPath = options.configuredPath.trim();

  if (configuredPath.length > 0) {
    candidates.push({ path: configuredPath, source: "setting" });
  }
  candidates.push({
    path: pathApi.join(options.extensionPath, "bin", target, binaryName),
    source: "bundled",
  });
  if (options.downloadedPath !== undefined) {
    candidates.push({ path: options.downloadedPath, source: "download" });
  }
  if (options.includePath === false) {
    return candidates;
  }
  for (const directory of pathEntries(options.platform, options.envPath)) {
    candidates.push({ path: pathApi.join(directory, binaryName), source: "path" });
  }

  return candidates;
}

function pathEntries(platform: string, envPath: string | undefined): string[] {
  if (envPath === undefined || envPath.length === 0) {
    return [];
  }
  const separator = platform === "win32" ? ";" : ":";
  return envPath.split(separator).filter((entry) => entry.length > 0);
}

function missingBinaryMessage(candidates: Candidate[]): string {
  const lines = [
    "Cannot find the nullscan-lsp language server. Checked:",
    ...candidates.map((candidate) => `  - ${candidate.path} (${sourceLabel(candidate.source)})`),
    'Set "nullscan.path" to an existing nullscan-lsp executable, or run "Nullscan: Redownload Server".',
  ];
  return lines.join("\n");
}

function unsupportedTargetMessage(platform: string, arch: string): string {
  return [
    `Unsupported platform/architecture: ${platform}-${arch}.`,
    "Nullscan ships prebuilt servers for linux, darwin and win32 on x64 and arm64.",
    'Set "nullscan.path" to a nullscan-lsp binary built for this machine.',
  ].join("\n");
}

function sourceLabel(source: BinarySource): string {
  switch (source) {
    case "setting":
      return "nullscan.path";
    case "bundled":
      return "bundled";
    case "download":
      return "downloaded";
    case "path":
      return "PATH";
  }
}
