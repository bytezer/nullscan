import { describe, expect, it } from "vitest";

import {
  parseServerVersion,
  resolveServerBinary,
  serverTarget,
  type ResolveBinaryResult,
} from "../src/binary";

const EXTENSION = "/extensions/vscode";
const BUNDLED = `${EXTENSION}/bin/linux-x64/nullscan-lsp`;

interface ResolveInput {
  existing: string[];
  configuredPath?: string;
  extensionPath?: string;
  platform?: string;
  arch?: string;
  envPath?: string;
}

function resolve(input: ResolveInput): ResolveBinaryResult {
  const {
    existing,
    configuredPath = "",
    extensionPath = EXTENSION,
    platform = "linux",
    arch = "x64",
    envPath = "",
  } = input;
  return resolveServerBinary({
    configuredPath,
    extensionPath,
    platform,
    arch,
    envPath,
    isExecutable: (candidate) => existing.includes(candidate),
  });
}

function expectResolved(result: ResolveBinaryResult): { path: string; source: string } {
  if (!("path" in result)) {
    throw new Error(`expected a binary, got: ${result.error}`);
  }
  return result;
}

function expectFailure(result: ResolveBinaryResult): {
  error: string;
  candidates: string[];
} {
  if (!("error" in result)) {
    throw new Error(`expected a failure, got: ${result.path}`);
  }
  return result;
}

describe("resolveServerBinary", () => {
  it("prefers nullscan.path over the bundled binary and PATH", () => {
    const configured = "/opt/nullscan/nullscan-lsp";
    const result = expectResolved(
      resolve({
        existing: [configured, BUNDLED, "/usr/local/bin/nullscan-lsp"],
        configuredPath: configured,
        envPath: "/usr/local/bin",
      }),
    );

    expect(result).toEqual({ path: configured, source: "setting" });
  });

  it("falls back to the bundled binary when the setting is empty", () => {
    const result = expectResolved(resolve({ existing: [BUNDLED] }));

    expect(result).toEqual({ path: BUNDLED, source: "bundled" });
  });

  it("falls back to the bundled binary when the setting does not exist", () => {
    const result = expectResolved(
      resolve({ existing: [BUNDLED], configuredPath: "/missing/nullscan-lsp" }),
    );

    expect(result).toEqual({ path: BUNDLED, source: "bundled" });
  });

  it("searches PATH in order after the bundled binary", () => {
    const result = expectResolved(
      resolve({
        existing: ["/second/nullscan-lsp", "/first/nullscan-lsp"],
        envPath: "/first:/second",
      }),
    );

    expect(result).toEqual({ path: "/first/nullscan-lsp", source: "path" });
  });

  it("reports every candidate in resolution order", () => {
    const configured = "/missing/nullscan-lsp";
    const result = expectFailure(
      resolve({
        existing: [],
        configuredPath: configured,
        envPath: "/one:/two",
      }),
    );

    expect(result.candidates).toEqual([
      configured,
      BUNDLED,
      "/one/nullscan-lsp",
      "/two/nullscan-lsp",
    ]);
    expect(result.error).toContain("Cannot find the nullscan-lsp language server");
    expect(result.error).toContain(`${configured} (nullscan.path)`);
    expect(result.error).toContain(`${BUNDLED} (bundled)`);
    expect(result.error).toContain("/one/nullscan-lsp (PATH)");
    expect(result.error).toContain('Set "nullscan.path"');
    expect(result.error.indexOf(configured)).toBeLessThan(result.error.indexOf(BUNDLED));
  });

  it("uses .exe for win32 candidates", () => {
    const result = expectFailure(
      resolve({
        existing: [],
        extensionPath: "C:\\extension",
        platform: "win32",
        arch: "x64",
      }),
    );

    expect(result.candidates[0]).toBe(
      "C:\\extension\\bin\\win32-x64\\nullscan-lsp.exe",
    );
  });

  it("allows nullscan.path even when no bundled target exists", () => {
    const configured = "/opt/nullscan/nullscan-lsp";
    const result = expectResolved(
      resolve({
        existing: [configured],
        configuredPath: configured,
        platform: "freebsd",
        arch: "x64",
      }),
    );

    expect(result).toEqual({ path: configured, source: "setting" });
  });

  it("fails clearly for an unsupported platform or architecture", () => {
    const platform = expectFailure(
      resolve({ existing: [], platform: "freebsd", arch: "x64" }),
    );
    expect(platform.error).toContain("Unsupported platform/architecture: freebsd-x64");
    expect(platform.candidates).toEqual([]);

    const arch = expectFailure(resolve({ existing: [], platform: "linux", arch: "ia32" }));
    expect(arch.error).toContain("Unsupported platform/architecture: linux-ia32");
  });
});

describe("serverTarget", () => {
  it("maps the supported platform and architecture pairs", () => {
    expect(serverTarget("linux", "x64")).toBe("linux-x64");
    expect(serverTarget("linux", "arm64")).toBe("linux-arm64");
    expect(serverTarget("darwin", "x64")).toBe("darwin-x64");
    expect(serverTarget("darwin", "arm64")).toBe("darwin-arm64");
    expect(serverTarget("win32", "x64")).toBe("win32-x64");
  });

  it("rejects unknown combinations", () => {
    expect(serverTarget("freebsd", "x64")).toBeUndefined();
    expect(serverTarget("linux", "ia32")).toBeUndefined();
  });
});

describe("parseServerVersion", () => {
  it("parses the --version output", () => {
    expect(parseServerVersion("nullscan-lsp 0.1.0\n")).toBe("0.1.0");
    expect(parseServerVersion("nullscan-lsp 0.2.0-beta.1")).toBe("0.2.0-beta.1");
    expect(parseServerVersion("nullscan-lsp 0.2.0-beta.1+build.5")).toBe(
      "0.2.0-beta.1+build.5",
    );
  });

  it("returns undefined for unexpected output", () => {
    expect(parseServerVersion("")).toBeUndefined();
    expect(parseServerVersion("nullscan 0.1.0")).toBeUndefined();
    expect(parseServerVersion("garbage")).toBeUndefined();
  });
});
