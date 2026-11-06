import { resolve } from "node:path";

import { describe, expect, it } from "vitest";

import { resolveExplicitConfigPath } from "../src/configPath";

describe("resolveExplicitConfigPath", () => {
  it("treats an empty or whitespace-only setting as unset", () => {
    expect(resolveExplicitConfigPath("", "/repo")).toEqual({ kind: "unset" });
    expect(resolveExplicitConfigPath("   ", "/repo")).toEqual({ kind: "unset" });
  });

  it("keeps an absolute path as it is", () => {
    expect(resolveExplicitConfigPath("/etc/nullscan.toml", "/repo")).toEqual({
      kind: "resolved",
      path: "/etc/nullscan.toml",
    });
  });

  it("resolves a relative path against the first workspace folder", () => {
    expect(resolveExplicitConfigPath("config/nullscan.toml", "/repo")).toEqual({
      kind: "resolved",
      path: resolve("/repo", "config/nullscan.toml"),
    });
  });

  it("reports a relative path it cannot anchor", () => {
    expect(resolveExplicitConfigPath("nullscan.toml", undefined)).toEqual({
      kind: "unresolved",
      configured: "nullscan.toml",
    });
  });
});
