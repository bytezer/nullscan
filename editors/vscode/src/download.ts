import { execFile } from "node:child_process";
import { createHash, randomUUID } from "node:crypto";
import { createReadStream, createWriteStream } from "node:fs";
import { chmod, mkdir, rename, rm, stat } from "node:fs/promises";
import { request as requestHttp, type IncomingMessage } from "node:http";
import { request as requestHttps } from "node:https";
import { dirname, join } from "node:path";
import { Transform, type TransformCallback } from "node:stream";
import { pipeline } from "node:stream/promises";

import { HttpsProxyAgent } from "https-proxy-agent";

import {
  parseServerVersion,
  serverTriple,
  serverVersionMatches,
} from "./binary";

/** GitHub release base; the version directory and asset are appended by `releaseUrl`. */
export const DEFAULT_RELEASE_BASE = "https://github.com/bytezer/nullscan/releases/download";

/** Name of the checksum manifest published next to every asset. */
export const CHECKSUMS_ASSET = "checksums.txt";

const MAX_REDIRECTS = 10;
const REQUEST_TIMEOUT_MS = 60_000;
const VERSION_TIMEOUT_MS = 10_000;
const MAX_CHECKSUMS_BYTES = 1024 * 1024;
const DOWNLOAD_BASE_ENV = "NULLSCAN_DOWNLOAD_BASE_URL";

/** Where a cached server binary lives plus the release coordinates it came from. */
export interface CachedServerLocation {
  path: string;
  triple: string;
  asset: string;
}

export interface DownloadServerOptions {
  /** Extension version; pins the release tag and the cache directory. */
  version: string;
  /** `context.globalStorageUri.fsPath`; the server is cached under `server/`. */
  cacheDir: string;
  /** Progress callback: bytes downloaded so far and total bytes (0 when unknown). */
  onProgress?: (downloaded: number, total: number) => void;
  /** Cancels the download and removes the partially written file. */
  signal?: AbortSignal | undefined;
  /** Receives one line per step; the extension forwards this to its output channel. */
  log?: (line: string) => void;
  /** Release base URL override; defaults to `NULLSCAN_DOWNLOAD_BASE_URL` then GitHub. */
  baseUrl?: string;
  /** `process.platform`, injectable for tests. */
  platform?: string;
  /** `process.arch`, injectable for tests. */
  arch?: string;
  /** `http.proxy` VS Code setting; empty means a direct connection. */
  proxy?: string;
  /** `http.proxyStrictSSL` VS Code setting. */
  strictSSL?: boolean;
  /** `--version` runner, injectable so tests do not need an executable. */
  runVersion?: (path: string) => Promise<string>;
}

export interface DownloadServerResult {
  path: string;
  /** `false` when a verified cache entry was reused. */
  downloaded: boolean;
}

/** Release target triple for `<platform>-<arch>`; see `serverTriple` in `binary.ts`. */
export function targetTriple(platform: string, arch: string): string | undefined {
  return serverTriple(platform, arch);
}

/** `<asset>` for a target triple; Windows assets carry an `.exe` suffix. */
export function assetName(triple: string): string {
  if (triple.trim().length === 0) {
    throw new Error("Cannot build a release asset name for an empty target triple.");
  }
  return triple.includes("windows") ? `nullscan-lsp-${triple}.exe` : `nullscan-lsp-${triple}`;
}

/** `https://github.com/bytezer/nullscan/releases/download/v<version>/<asset>`. */
export function releaseUrl(
  version: string,
  asset: string,
  baseUrl: string = DEFAULT_RELEASE_BASE,
): string {
  const trimmedVersion = version.trim();
  if (trimmedVersion.length === 0) {
    throw new Error("Cannot build a release URL without a version.");
  }
  if (asset.trim().length === 0) {
    throw new Error("Cannot build a release URL without an asset name.");
  }
  return `${normalizeBaseUrl(baseUrl)}/v${trimmedVersion}/${asset}`;
}

/**
 * Release base URL from an explicit override, the test-only environment variable, or
 * GitHub. Production URLs are `https:`; `http:` is only accepted through an override.
 */
export function resolveReleaseBase(override?: string): string {
  const value = override ?? process.env[DOWNLOAD_BASE_ENV];
  if (value === undefined || value.trim().length === 0) {
    return DEFAULT_RELEASE_BASE;
  }
  return normalizeBaseUrl(value.trim());
}

/** Parses `<sha256>  <asset>` lines into a lowercase-hash map keyed by asset name. */
export function parseChecksums(text: string): Map<string, string> {
  const checksums = new Map<string, string>();
  text.split(/\r?\n/).forEach((raw, index) => {
    const line = raw.trim();
    if (line.length === 0 || line.startsWith("#")) {
      return;
    }
    const match = /^([0-9a-fA-F]{64})\s+\*?(\S.*)$/.exec(line);
    const hash = match?.[1];
    const asset = match?.[2];
    if (hash === undefined || asset === undefined) {
      throw new Error(`Invalid checksums.txt line ${index + 1}: ${JSON.stringify(raw)}`);
    }
    checksums.set(asset.trim(), hash.toLowerCase());
  });
  return checksums;
}

/** `<cacheDir>/server/<version>/<asset>`; one directory per extension version. */
export function serverCachePath(cacheDir: string, version: string, asset: string): string {
  return join(cacheDir, "server", version.trim(), asset);
}

/** Cache coordinates for this machine, or `undefined` for an unsupported target. */
export function cachedServerLocation(
  cacheDir: string,
  version: string,
  platform: string,
  arch: string,
): CachedServerLocation | undefined {
  const triple = targetTriple(platform, arch);
  if (triple === undefined) {
    return undefined;
  }
  const asset = assetName(triple);
  return { path: serverCachePath(cacheDir, version, asset), triple, asset };
}

/** Deletes a cached binary so the next resolution downloads a fresh copy. */
export async function removeCachedServer(location: CachedServerLocation): Promise<void> {
  await rm(location.path, { force: true });
}

/**
 * Downloads, verifies and caches `nullscan-lsp` for this machine. Reuses an existing
 * cache entry only when its SHA-256 matches the published `checksums.txt`.
 */
export async function downloadServer(
  options: DownloadServerOptions,
): Promise<DownloadServerResult> {
  const platform = options.platform ?? process.platform;
  const arch = options.arch ?? process.arch;
  const log = options.log ?? (() => {});
  const location = cachedServerLocation(options.cacheDir, options.version, platform, arch);
  if (location === undefined) {
    throw new Error(
      `Unsupported platform/architecture: ${platform}-${arch}. Nullscan has no prebuilt language server for this machine; set "nullscan.path" instead.`,
    );
  }

  const baseUrl = resolveReleaseBase(options.baseUrl);
  const fetch = fetchOptions(options);
  log(`Checking ${releaseUrl(options.version, CHECKSUMS_ASSET, baseUrl)}.`);
  const checksums = parseChecksums(
    await fetchText(releaseUrl(options.version, CHECKSUMS_ASSET, baseUrl), fetch),
  );
  const expected = checksums.get(location.asset);
  if (expected === undefined) {
    throw new Error(`${CHECKSUMS_ASSET} does not list ${location.asset}.`);
  }

  const cached = await verifyCached(location.path, expected, platform, log);
  if (cached) {
    return { path: location.path, downloaded: false };
  }

  const url = releaseUrl(options.version, location.asset, baseUrl);
  log(`Downloading ${url}.`);
  const tempPath = `${location.path}.${randomUUID()}.tmp`;
  await mkdir(dirname(location.path), { recursive: true });
  const response = await openResponse(url, fetch);

  let actual: string;
  try {
    actual = await writeTemp(response, tempPath, options, fetch.signal);
    if (actual !== expected) {
      throw new Error(
        `Checksum mismatch for ${location.asset}: expected ${expected}, got ${actual}. The download was discarded.`,
      );
    }
    if (platform !== "win32") {
      await chmod(tempPath, 0o755);
    }
    throwIfAborted(fetch.signal);
    await rename(tempPath, location.path);
  } catch (error) {
    await rm(tempPath, { force: true });
    throw error;
  }

  log(`Downloaded ${location.asset} to ${location.path}.`);
  await verifyDownloadedVersion(location.path, options);
  return { path: location.path, downloaded: true };
}

interface FetchOptions {
  signal: AbortSignal | undefined;
  proxy: string | undefined;
  strictSSL: boolean;
  headers: Record<string, string>;
}

function fetchOptions(options: DownloadServerOptions): FetchOptions {
  return {
    signal: options.signal,
    proxy: options.proxy,
    strictSSL: options.strictSSL ?? true,
    headers: { "user-agent": `nullscan-vscode/${options.version}` },
  };
}

function normalizeBaseUrl(value: string): string {
  const trimmed = value.trim().replace(/\/+$/, "");
  let url: URL;
  try {
    url = new URL(trimmed);
  } catch {
    throw new Error(`Invalid release base URL: ${value}`);
  }
  if (url.protocol !== "https:" && url.protocol !== "http:") {
    throw new Error(`Release base URL must use http or https: ${value}`);
  }
  return trimmed;
}

async function verifyCached(
  path: string,
  expected: string,
  platform: string,
  log: (line: string) => void,
): Promise<boolean> {
  if (!(await fileExists(path))) {
    return false;
  }
  const actual = await sha256File(path);
  if (actual === expected) {
    if (platform !== "win32") {
      await chmod(path, 0o755);
    }
    log(`Reusing cached ${path} (checksum matches).`);
    return true;
  }
  log(`Cached ${path} does not match checksums.txt; downloading it again.`);
  await rm(path, { force: true });
  return false;
}

async function writeTemp(
  response: IncomingMessage,
  tempPath: string,
  options: DownloadServerOptions,
  signal: AbortSignal | undefined,
): Promise<string> {
  const hash = createHash("sha256");
  const total = contentLength(response);
  let downloaded = 0;
  const meter = new Transform({
    transform(chunk: Buffer, _encoding: BufferEncoding, callback: TransformCallback) {
      hash.update(chunk);
      downloaded += chunk.length;
      options.onProgress?.(downloaded, total);
      callback(null, chunk);
    },
  });

  try {
    await pipeline(response, meter, createWriteStream(tempPath, { mode: 0o600 }), { signal });
  } catch (error) {
    if (signal?.aborted === true) {
      throw abortError();
    }
    throw error;
  }
  throwIfAborted(signal);
  return hash.digest("hex");
}

function contentLength(response: IncomingMessage): number {
  const header = response.headers["content-length"];
  const raw = Array.isArray(header) ? header[0] : header;
  const parsed = Number(raw ?? 0);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : 0;
}

async function fetchText(url: string, options: FetchOptions): Promise<string> {
  const response = await openResponse(url, options);
  let text = "";
  for await (const chunk of response) {
    throwIfAborted(options.signal);
    text += Buffer.isBuffer(chunk) ? chunk.toString("utf8") : String(chunk);
    if (text.length > MAX_CHECKSUMS_BYTES) {
      response.destroy();
      throw new Error(`${CHECKSUMS_ASSET} is unexpectedly large.`);
    }
  }
  return text;
}

async function openResponse(url: string, options: FetchOptions): Promise<IncomingMessage> {
  let current = url;
  for (let redirects = 0; redirects <= MAX_REDIRECTS; redirects += 1) {
    const response = await requestOnce(current, options);
    const status = response.statusCode ?? 0;
    const location = response.headers.location;
    if (status >= 300 && status < 400) {
      response.resume();
      if (location === undefined) {
        throw new Error(`HTTP ${status} without a redirect location for ${current}.`);
      }
      current = new URL(location, current).toString();
      continue;
    }
    if (status >= 400 || status === 0) {
      response.resume();
      throw new Error(`HTTP ${status === 0 ? "error" : status} while downloading ${current}.`);
    }
    return response;
  }
  throw new Error(`Too many redirects while downloading ${url}.`);
}

function requestOnce(url: string, options: FetchOptions): Promise<IncomingMessage> {
  return new Promise((resolve, reject) => {
    const target = new URL(url);
    if (target.protocol !== "https:" && target.protocol !== "http:") {
      reject(new Error(`Unsupported download URL: ${url}`));
      return;
    }
    const agent = agentFor(target, options);
    const common = {
      method: "GET" as const,
      headers: options.headers,
      agent,
      signal: options.signal,
      timeout: REQUEST_TIMEOUT_MS,
    };
    const request =
      target.protocol === "https:"
        ? requestHttps(target, { ...common, rejectUnauthorized: options.strictSSL })
        : requestHttp(target, common);
    request.on("response", resolve);
    request.on("error", reject);
    request.on("timeout", () => {
      request.destroy(new Error(`Timed out after ${REQUEST_TIMEOUT_MS} ms while downloading ${url}.`));
    });
    request.end();
  });
}

function agentFor(target: URL, options: FetchOptions): HttpsProxyAgent<string> | undefined {
  const proxy = options.proxy?.trim();
  if (target.protocol !== "https:" || proxy === undefined || proxy.length === 0) {
    return undefined;
  }
  return new HttpsProxyAgent(proxy, { rejectUnauthorized: options.strictSSL });
}

async function verifyDownloadedVersion(
  path: string,
  options: DownloadServerOptions,
): Promise<void> {
  const run = options.runVersion ?? runVersionCommand;
  let output: string;
  try {
    output = await run(path);
  } catch (error) {
    await rm(path, { force: true });
    throw new Error(
      `The downloaded nullscan-lsp could not be executed (${messageOf(error)}). The file was deleted.`,
    );
  }

  const actual = parseServerVersion(output);
  if (actual === undefined || !serverVersionMatches(options.version, actual)) {
    await rm(path, { force: true });
    throw new Error(
      `The downloaded nullscan-lsp reports ${actual ?? "an unknown version"} but this extension is ${options.version}. The file was deleted.`,
    );
  }
}

function runVersionCommand(path: string): Promise<string> {
  return new Promise((resolve, reject) => {
    execFile(path, ["--version"], { timeout: VERSION_TIMEOUT_MS, windowsHide: true }, (error, stdout) => {
      if (error !== null) {
        reject(error);
        return;
      }
      resolve(stdout);
    });
  });
}

async function sha256File(path: string): Promise<string> {
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(path)) {
    hash.update(chunk);
  }
  return hash.digest("hex");
}

async function fileExists(path: string): Promise<boolean> {
  try {
    await stat(path);
    return true;
  } catch {
    return false;
  }
}

function throwIfAborted(signal: AbortSignal | undefined): void {
  if (signal?.aborted === true) {
    throw abortError();
  }
}

function abortError(): Error {
  const error = new Error("The nullscan server download was cancelled.");
  error.name = "AbortError";
  return error;
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
