import { describe, expect, it } from "vitest";

import {
  groupByFile,
  groupByRule,
  hotFiles,
  severityLevelOf,
  severityOf,
  severityTotals,
  topRules,
  toIssues,
  worstSeverity,
  type DiagnosticLike,
  type Issue,
} from "../src/aggregate";

function diagnostic(overrides: Partial<DiagnosticLike> = {}): DiagnosticLike {
  return {
    range: { start: { line: 0, character: 0 }, end: { line: 0, character: 4 } },
    message: "boom",
    severity: 1,
    code: "no-console",
    ...overrides,
  };
}

function issue(overrides: Partial<Issue> = {}): Issue {
  return {
    uri: "file:///repo/src/a.ts",
    file: "/repo/src/a.ts",
    line: 1,
    column: 1,
    endLine: 1,
    endColumn: 5,
    rule: "no-console",
    severity: "warning",
    message: "boom",
    ...overrides,
  };
}

describe("toIssues", () => {
  it("converts 0-based positions to 1-based display positions", () => {
    const [mapped] = toIssues([
      [
        "file:///repo/src/a.ts",
        [
          diagnostic({
            range: { start: { line: 4, character: 2 }, end: { line: 6, character: 9 } },
          }),
        ],
      ],
    ]);

    expect(mapped).toEqual({
      uri: "file:///repo/src/a.ts",
      file: "/repo/src/a.ts",
      line: 5,
      column: 3,
      endLine: 7,
      endColumn: 10,
      rule: "no-console",
      severity: "warning",
      message: "boom",
    });
  });

  it("keeps input order across files", () => {
    const issues = toIssues([
      ["file:///repo/b.ts", [diagnostic({ message: "second" }), diagnostic({ message: "third" })]],
      ["file:///repo/a.ts", [diagnostic({ message: "first" })]],
    ]);

    expect(issues.map((entry) => entry.message)).toEqual(["second", "third", "first"]);
  });

  it("returns an empty list for empty input", () => {
    expect(toIssues([])).toEqual([]);
  });

  it("maps unknown severities and missing rules", () => {
    const [mapped] = toIssues([["file:///repo/a.ts", [diagnostic({ severity: 99, code: undefined })]]]);

    expect(mapped?.severity).toBe("info");
    expect(mapped?.rule).toBe("unknown");
  });

  it("reads rule ids from object codes", () => {
    const [mapped] = toIssues([
      ["file:///repo/a.ts", [diagnostic({ code: { value: "no-eval" } })]],
    ]);

    expect(mapped?.rule).toBe("no-eval");
  });

  it("keeps non-file URIs unchanged", () => {
    const [mapped] = toIssues([["untitled:Untitled-1", [diagnostic()]]]);

    expect(mapped?.file).toBe("untitled:Untitled-1");
  });
});

describe("severityOf", () => {
  it("maps VS Code severities", () => {
    expect(severityOf(0)).toBe("error");
    expect(severityOf(1)).toBe("warning");
    expect(severityOf(2)).toBe("info");
    expect(severityOf(3)).toBe("info");
  });

  it("falls back to info for missing or unknown severities", () => {
    expect(severityOf(undefined)).toBe("info");
    expect(severityOf(42)).toBe("info");
  });
});

describe("groupByRule", () => {
  it("orders groups by count, then by rule id", () => {
    const groups = groupByRule([
      issue({ rule: "no-console" }),
      issue({ rule: "no-console" }),
      issue({ rule: "no-eval" }),
      issue({ rule: "no-var" }),
      issue({ rule: "no-var" }),
      issue({ rule: "no-eval" }),
    ]);

    expect(groups.map((group) => [group.rule, group.count])).toEqual([
      ["no-console", 2],
      ["no-eval", 2],
      ["no-var", 2],
    ]);
  });

  it("breaks count ties with rule id ascending", () => {
    const groups = groupByRule([issue({ rule: "zeta" }), issue({ rule: "alpha" })]);

    expect(groups.map((group) => group.rule)).toEqual(["alpha", "zeta"]);
  });

  it("records the worst severity of each group", () => {
    const groups = groupByRule([
      issue({ rule: "mixed", severity: "info" }),
      issue({ rule: "mixed", severity: "error" }),
      issue({ rule: "mixed", severity: "warning" }),
    ]);

    expect(groups[0]?.severity).toBe("error");
  });

  it("returns no groups for no issues", () => {
    expect(groupByRule([])).toEqual([]);
  });
});

describe("groupByFile", () => {
  it("orders mixes of files by count, then path", () => {
    const groups = groupByFile([
      issue({ file: "/repo/src/b.ts" }),
      issue({ file: "/repo/src/a.ts" }),
      issue({ file: "/repo/src/a.ts" }),
      issue({ file: "/repo/src/c.ts" }),
    ]);

    expect(groups.map((group) => [group.file, group.count])).toEqual([
      ["/repo/src/a.ts", 2],
      ["/repo/src/b.ts", 1],
      ["/repo/src/c.ts", 1],
    ]);
  });

  it("keeps the issues of each file", () => {
    const groups = groupByFile([
      issue({ file: "/repo/src/a.ts", message: "one" }),
      issue({ file: "/repo/src/a.ts", message: "two" }),
    ]);

    expect(groups[0]?.issues.map((entry) => entry.message)).toEqual(["one", "two"]);
  });

  it("returns no groups for no issues", () => {
    expect(groupByFile([])).toEqual([]);
  });
});

describe("severityTotals", () => {
  it("counts every level, including zeroes", () => {
    const totals = severityTotals([
      issue({ severity: "error" }),
      issue({ severity: "info" }),
      issue({ severity: "info" }),
    ]);

    expect(totals).toEqual({ error: 1, warning: 0, info: 2 });
  });

  it("returns zeroes for no issues", () => {
    expect(severityTotals([])).toEqual({ error: 0, warning: 0, info: 0 });
  });
});

describe("worstSeverity", () => {
  it("returns error, warning and info for the worst level present", () => {
    expect(worstSeverity([issue({ severity: "info" }), issue({ severity: "error" })])).toBe("error");
    expect(worstSeverity([issue({ severity: "info" }), issue({ severity: "warning" })])).toBe(
      "warning",
    );
    expect(worstSeverity([issue({ severity: "info" })])).toBe("info");
  });

  it("returns clean for no issues", () => {
    expect(worstSeverity([])).toBe("clean");
  });
});

describe("severityLevelOf", () => {
  it("mirrors worstSeverity for precomputed totals", () => {
    expect(severityLevelOf({ error: 0, warning: 2, info: 1 })).toBe("warning");
    expect(severityLevelOf({ error: 0, warning: 0, info: 0 })).toBe("clean");
  });
});

describe("topRules and hotFiles", () => {
  it("limits to n entries, reusing group order", () => {
    const issues = [
      issue({ rule: "b", file: "/repo/b.ts" }),
      issue({ rule: "a", file: "/repo/a.ts" }),
      issue({ rule: "c", file: "/repo/c.ts" }),
    ];

    expect(topRules(issues, 2).map((group) => group.rule)).toEqual(["a", "b"]);
    expect(hotFiles(issues, 1).map((group) => group.file)).toEqual(["/repo/a.ts"]);
  });

  it("returns an empty list for n <= 0", () => {
    const issues = [issue()];

    expect(topRules(issues, 0)).toEqual([]);
    expect(topRules(issues, -3)).toEqual([]);
    expect(hotFiles(issues, 0)).toEqual([]);
  });

  it("returns fewer entries than n", () => {
    expect(topRules([issue()], 10)).toHaveLength(1);
    expect(hotFiles([issue()], 10)).toHaveLength(1);
  });
});
