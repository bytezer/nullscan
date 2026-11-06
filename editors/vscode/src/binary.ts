import { accessSync, constants } from "node:fs";
import { posix, win32 } from "node:path";

/** Where a resolved server binary came from. */
export type BinarySource = "setting" | "bundled" | "path";

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
  /** Existence check, injectable for tests. */
  isExecutable?: (candidate: string) => boolean;
}

interface Candidate {
  path: string;
  source: BinarySource;
}

const PLATFORMS = ["linux", "darwin", "win32"] as const;
const ARCHITECTURES = ["x64", "arm64"] as const;

/** e.g. `linux-x64`; `undefined` for a combination without a prebuilt server. */
export function serverTarget(platform: string, arch: string): string | undefined {
  if (!isPlatform(platform) || !isArch(arch)) {
    return undefined;
  }
  return `${platform}-${arch}`;
}

/** Resolves the server in `nullscan.path` → bundled binary → `PATH` order. */
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

/** Default existence check: the candidate must be present and executable. */
export function isExecutableFile(candidate: string): boolean {
  try {
    accessSync(candidate, constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

function isPlatform(value: string): value is (typeof PLATFORMS)[number] {
  return (PLATFORMS as readonly string[]).includes(value);
}

function isArch(value: string): value is (typeof ARCHITECTURES)[number] {
  return (ARCHITECTURES as readonly string[]).includes(value);
}

function candidatePaths(
  options: ResolveBinaryOptions,
  target: string,
): Candidate[] {
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
    'Set "nullscan.path" to an existing nullscan-lsp executable or install nullscan.',
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
    case "path":
      return "PATH";
  }
}
