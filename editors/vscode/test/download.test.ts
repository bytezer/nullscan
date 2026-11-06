import { createHash } from "node:crypto";
import { mkdir, mkdtemp, readdir, rm, stat, writeFile } from "node:fs/promises";
import { createServer, type ServerResponse } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  assetName,
  downloadServer,
  parseChecksums,
  releaseUrl,
  resolveReleaseBase,
  serverCachePath,
  targetTriple,
} from "../src/download";

const VERSION = "0.1.0-beta.1";
const TRIPLE = "x86_64-unknown-linux-gnu";
const ASSET = `nullscan-lsp-${TRIPLE}`;
const BINARY = '#!/bin/sh\necho "nullscan-lsp 0.1.0"\n';
const CHECKSUMS = `/releases/download/v${VERSION}/checksums.txt`;
const ASSET_PATH = `/releases/download/v${VERSION}/${ASSET}`;
const CDN_PATH = `/cdn/${ASSET}`;

let cacheRoot: string;
let servers: Array<() => Promise<void>>;

beforeEach(async () => {
  cacheRoot = await mkdtemp(join(tmpdir(), "nullscan-download-"));
  servers = [];
});

afterEach(async () => {
  vi.unstubAllEnvs();
  await Promise.all(servers.map((close) => close()));
  await rm(cacheRoot, { recursive: true, force: true });
});

interface ReleaseServerOptions {
  binary?: string;
  checksums?: string;
  redirectAsset?: boolean;
  hangAsset?: boolean;
}

interface TestServer {
  origin: string;
  requests: string[];
  close: () => Promise<void>;
}

async function startReleaseServer(options: ReleaseServerOptions = {}): Promise<TestServer> {
  const binary = options.binary ?? BINARY;
  const requests: string[] = [];
  const server = createServer((request, response) => {
    requests.push(request.url ?? "");
    route(request.url ?? "", response, { ...options, binary });
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  const port = typeof address === "object" && address !== null ? address.port : 0;
  const close = async (): Promise<void> => {
    server.closeAllConnections();
    await new Promise<void>((resolve) => server.close(() => resolve()));
  };
  servers.push(close);
  return { origin: `http://127.0.0.1:${port}`, requests, close };
}

function route(url: string, response: ServerResponse, options: ReleaseServerOptions): void {
  if (url === CHECKSUMS) {
    const checksums = options.checksums ?? checksumsFor(ASSET, options.binary ?? BINARY);
    response.writeHead(200, { "content-type": "text/plain" });
    response.end(checksums);
    return;
  }
  if (url === ASSET_PATH) {
    if (options.redirectAsset === true) {
      response.writeHead(302, { location: CDN_PATH });
      response.end();
      return;
    }
    if (options.hangAsset === true) {
      response.writeHead(200, { "content-length": String(BINARY.length * 100) });
      response.write(BINARY.slice(0, 4));
      return;
    }
  }
  if (url === ASSET_PATH || url === CDN_PATH) {
    const binary = options.binary ?? BINARY;
    response.writeHead(200, { "content-length": String(Buffer.byteLength(binary)) });
    response.end(binary);
    return;
  }
  response.writeHead(404);
  response.end("not found");
}

function checksumsFor(asset: string, content: string): string {
  return `${createHash("sha256").update(content).digest("hex")}  ${asset}\n`;
}

function baseUrl(origin: string): string {
  return `${origin}/releases/download`;
}

function cachePath(): string {
  return serverCachePath(cacheRoot, VERSION, ASSET);
}

async function seedCache(content: string): Promise<void> {
  const path = cachePath();
  await mkdir(join(cacheRoot, "server", VERSION), { recursive: true });
  await writeFile(path, content);
}

async function tempFiles(directory: string): Promise<string[]> {
  try {
    return (await readdir(directory)).filter((name) => name.endsWith(".tmp"));
  } catch {
    return [];
  }
}

describe("targetTriple", () => {
  it("maps every supported platform and architecture pair", () => {
    expect(targetTriple("linux", "x64")).toBe("x86_64-unknown-linux-gnu");
    expect(targetTriple("linux", "arm64")).toBe("aarch64-unknown-linux-gnu");
    expect(targetTriple("darwin", "x64")).toBe("x86_64-apple-darwin");
    expect(targetTriple("darwin", "arm64")).toBe("aarch64-apple-darwin");
    expect(targetTriple("win32", "x64")).toBe("x86_64-pc-windows-msvc");
  });

  it("rejects unknown combinations", () => {
    expect(targetTriple("freebsd", "x64")).toBeUndefined();
    expect(targetTriple("win32", "arm64")).toBeUndefined();
    expect(targetTriple("linux", "ia32")).toBeUndefined();
  });
});

describe("assetName", () => {
  it("uses raw binary names with an .exe suffix on Windows", () => {
    expect(assetName("x86_64-unknown-linux-gnu")).toBe("nullscan-lsp-x86_64-unknown-linux-gnu");
    expect(assetName("aarch64-apple-darwin")).toBe("nullscan-lsp-aarch64-apple-darwin");
    expect(assetName("x86_64-pc-windows-msvc")).toBe(
      "nullscan-lsp-x86_64-pc-windows-msvc.exe",
    );
  });

  it("rejects an empty triple", () => {
    expect(() => assetName("  ")).toThrow(/empty target triple/);
  });
});

describe("parseChecksums", () => {
  it("parses sha256sum lines, comments and blank lines", () => {
    const text = [
      "# release checksums",
      "",
      `${"A".repeat(64)}  nullscan-lsp-x86_64-unknown-linux-gnu`,
      `${"b".repeat(64)} *nullscan-x86_64-unknown-linux-gnu\r`,
    ].join("\n");

    const checksums = parseChecksums(text);

    expect(checksums.size).toBe(2);
    expect(checksums.get("nullscan-lsp-x86_64-unknown-linux-gnu")).toBe("a".repeat(64));
    expect(checksums.get("nullscan-x86_64-unknown-linux-gnu")).toBe("b".repeat(64));
  });

  it("returns an empty map for an empty manifest", () => {
    expect(parseChecksums("")).toEqual(new Map());
  });

  it("rejects malformed lines with the line number", () => {
    expect(() => parseChecksums("not-a-hash  asset\n")).toThrow(/line 1/);
    expect(() => parseChecksums(`${"a".repeat(64)}\n`)).toThrow(/line 1/);
  });
});

describe("releaseUrl", () => {
  it("builds GitHub release URLs per version and asset", () => {
    expect(releaseUrl(VERSION, ASSET)).toBe(
      `https://github.com/bytezer/nullscan/releases/download/v${VERSION}/${ASSET}`,
    );
  });

  it("honours a base URL override", () => {
    expect(releaseUrl(VERSION, "checksums.txt", "http://127.0.0.1:9000/releases/")).toBe(
      `http://127.0.0.1:9000/releases/v${VERSION}/checksums.txt`,
    );
  });

  it("rejects missing versions and unsupported protocols", () => {
    expect(() => releaseUrl("", ASSET)).toThrow(/without a version/);
    expect(() => releaseUrl(VERSION, ASSET, "ftp://example.com/releases")).toThrow(
      /http or https/,
    );
  });
});

describe("resolveReleaseBase", () => {
  it("defaults to the GitHub release base", () => {
    vi.stubEnv("NULLSCAN_DOWNLOAD_BASE_URL", "");
    expect(resolveReleaseBase()).toBe(
      "https://github.com/bytezer/nullscan/releases/download",
    );
  });

  it("reads the test-only environment override", () => {
    vi.stubEnv("NULLSCAN_DOWNLOAD_BASE_URL", "http://127.0.0.1:9000/releases/");
    expect(resolveReleaseBase()).toBe("http://127.0.0.1:9000/releases");
  });

  it("lets an explicit override win over the environment", () => {
    vi.stubEnv("NULLSCAN_DOWNLOAD_BASE_URL", "http://127.0.0.1:9000/releases");
    expect(resolveReleaseBase("https://mirror.example.com/nullscan")).toBe(
      "https://mirror.example.com/nullscan",
    );
  });
});

describe("downloadServer", () => {
  it("downloads, verifies and caches the raw binary", async () => {
    const server = await startReleaseServer();
    const progress: Array<{ downloaded: number; total: number }> = [];

    const result = await downloadServer({
      version: VERSION,
      cacheDir: cacheRoot,
      baseUrl: baseUrl(server.origin),
      platform: "linux",
      arch: "x64",
      runVersion: async () => "nullscan-lsp 0.1.0\n",
      onProgress: (downloaded, total) => progress.push({ downloaded, total }),
    });

    expect(result).toEqual({ path: cachePath(), downloaded: true });
    expect(server.requests).toEqual([CHECKSUMS, ASSET_PATH]);
    expect(progress.at(-1)).toEqual({ downloaded: BINARY.length, total: BINARY.length });
    if (process.platform !== "win32") {
      expect((await stat(cachePath())).mode & 0o777).toBe(0o755);
    }
    await expect(readdir(join(cacheRoot, "server", VERSION))).resolves.toEqual([ASSET]);
  });

  it.skipIf(process.platform === "win32")(
    "runs the downloaded binary with --version before accepting it",
    async () => {
      const server = await startReleaseServer();

      const result = await downloadServer({
        version: VERSION,
        cacheDir: cacheRoot,
        baseUrl: baseUrl(server.origin),
        platform: "linux",
        arch: "x64",
      });

      expect(result.downloaded).toBe(true);
    },
  );

  it("reuses a cached binary whose checksum matches without downloading it again", async () => {
    await seedCache(BINARY);
    const server = await startReleaseServer();
    vi.stubEnv("NULLSCAN_DOWNLOAD_BASE_URL", baseUrl(server.origin));

    const result = await downloadServer({
      version: VERSION,
      cacheDir: cacheRoot,
      platform: "linux",
      arch: "x64",
      runVersion: async () => "nullscan-lsp 0.1.0\n",
    });

    expect(result).toEqual({ path: cachePath(), downloaded: false });
    expect(server.requests).toEqual([CHECKSUMS]);
  });

  it("downloads again when the cached binary does not match", async () => {
    await seedCache("corrupted");
    const server = await startReleaseServer();

    const result = await downloadServer({
      version: VERSION,
      cacheDir: cacheRoot,
      baseUrl: baseUrl(server.origin),
      platform: "linux",
      arch: "x64",
      runVersion: async () => "nullscan-lsp 0.1.0\n",
    });

    expect(result.downloaded).toBe(true);
    expect(server.requests).toEqual([CHECKSUMS, ASSET_PATH]);
  });

  it("follows a redirect to the CDN", async () => {
    const server = await startReleaseServer({ redirectAsset: true });

    const result = await downloadServer({
      version: VERSION,
      cacheDir: cacheRoot,
      baseUrl: baseUrl(server.origin),
      platform: "linux",
      arch: "x64",
      runVersion: async () => "nullscan-lsp 0.1.0\n",
    });

    expect(result.downloaded).toBe(true);
    expect(server.requests).toEqual([CHECKSUMS, ASSET_PATH, CDN_PATH]);
  });

  it("removes the temporary file on a checksum mismatch", async () => {
    const server = await startReleaseServer({ checksums: checksumsFor(ASSET, "other") });

    await expect(
      downloadServer({
        version: VERSION,
        cacheDir: cacheRoot,
        baseUrl: baseUrl(server.origin),
        platform: "linux",
        arch: "x64",
        runVersion: async () => "nullscan-lsp 0.1.0\n",
      }),
    ).rejects.toThrow(/Checksum mismatch/);

    await expect(readdir(join(cacheRoot, "server", VERSION))).resolves.toEqual([]);
  });

  it("cancels an in-flight download and removes the temporary file", async () => {
    const server = await startReleaseServer({ hangAsset: true });
    const controller = new AbortController();

    await expect(
      downloadServer({
        version: VERSION,
        cacheDir: cacheRoot,
        baseUrl: baseUrl(server.origin),
        platform: "linux",
        arch: "x64",
        signal: controller.signal,
        runVersion: async () => "nullscan-lsp 0.1.0\n",
        onProgress: () => controller.abort(),
      }),
    ).rejects.toThrow(/cancelled/);

    await expect(readdir(join(cacheRoot, "server", VERSION))).resolves.toEqual([]);
    expect(await tempFiles(join(cacheRoot, "server", VERSION))).toEqual([]);
  });

  it("deletes the binary when its version does not match the extension", async () => {
    const server = await startReleaseServer();

    await expect(
      downloadServer({
        version: VERSION,
        cacheDir: cacheRoot,
        baseUrl: baseUrl(server.origin),
        platform: "linux",
        arch: "x64",
        runVersion: async () => "nullscan-lsp 0.9.0\n",
      }),
    ).rejects.toThrow(/reports 0\.9\.0/);

    await expect(readdir(join(cacheRoot, "server", VERSION))).resolves.toEqual([]);
  });

  it("rejects a manifest that does not list the asset", async () => {
    const server = await startReleaseServer({ checksums: checksumsFor("other-asset", BINARY) });

    await expect(
      downloadServer({
        version: VERSION,
        cacheDir: cacheRoot,
        baseUrl: baseUrl(server.origin),
        platform: "linux",
        arch: "x64",
        runVersion: async () => "nullscan-lsp 0.1.0\n",
      }),
    ).rejects.toThrow(/does not list/);
  });

  it("rejects unsupported platforms before any network call", async () => {
    await expect(
      downloadServer({
        version: VERSION,
        cacheDir: cacheRoot,
        platform: "freebsd",
        arch: "x64",
      }),
    ).rejects.toThrow(/Unsupported platform\/architecture/);
  });
});
