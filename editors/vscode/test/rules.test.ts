import { describe, expect, it } from "vitest";

import {
  DOCS_BASE_URL,
  docsUrlOf,
  indexRules,
  ruleIdOfArgument,
  titleOf,
  whyOf,
  type RuleInfo,
} from "../src/rules";

function rule(overrides: Partial<RuleInfo> = {}): RuleInfo {
  return {
    id: "no-console",
    title: "No console statements",
    description: "Reports console calls.",
    why: "Console output is not structured logging.",
    severity: "warning",
    docs_url: `${DOCS_BASE_URL}#no-console`,
    ...overrides,
  };
}

describe("indexRules", () => {
  it("indexes the catalog by rule id in registry order", () => {
    const rules = indexRules([
      rule(),
      rule({ id: "eqeqeq", title: "Require strict equality" }),
    ]);

    expect([...rules.keys()]).toEqual(["no-console", "eqeqeq"]);
    expect(rules.get("eqeqeq")?.title).toBe("Require strict equality");
  });

  it("keeps the last entry for a duplicated id", () => {
    const rules = indexRules([rule(), rule({ title: "Later title" })]);

    expect(rules.size).toBe(1);
    expect(rules.get("no-console")?.title).toBe("Later title");
  });
});

describe("catalog lookups", () => {
  const rules = indexRules([rule()]);

  it("returns the human values for a known rule", () => {
    expect(titleOf(rules, "no-console")).toBe("No console statements");
    expect(whyOf(rules, "no-console")).toBe("Console output is not structured logging.");
    expect(docsUrlOf(rules, "no-console")).toBe(`${DOCS_BASE_URL}#no-console`);
  });

  it("falls back to the id, an empty rationale and the generic page", () => {
    expect(titleOf(rules, "unknown-rule")).toBe("unknown-rule");
    expect(whyOf(rules, "unknown-rule")).toBe("");
    expect(docsUrlOf(rules, "unknown-rule")).toBe(DOCS_BASE_URL);
  });

  it("falls back the same way when the catalog is empty", () => {
    const empty = indexRules([]);

    expect(titleOf(empty, "no-console")).toBe("no-console");
    expect(whyOf(empty, "no-console")).toBe("");
    expect(docsUrlOf(empty, "no-console")).toBe(DOCS_BASE_URL);
  });
});

describe("ruleIdOfArgument", () => {
  it("accepts a tree node with a rule field", () => {
    expect(ruleIdOfArgument({ kind: "rule", rule: "no-console", count: 2 })).toBe(
      "no-console",
    );
  });

  it("accepts a plain string from the palette", () => {
    expect(ruleIdOfArgument("no-console")).toBe("no-console");
  });

  it("ignores missing, blank and unrelated arguments", () => {
    expect(ruleIdOfArgument(undefined)).toBeUndefined();
    expect(ruleIdOfArgument("  ")).toBeUndefined();
    expect(ruleIdOfArgument({ kind: "file" })).toBeUndefined();
    expect(ruleIdOfArgument(42)).toBeUndefined();
  });
});
