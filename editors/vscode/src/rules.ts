/**
 * The `nullscan/rules` payload and lookups over it.
 *
 * Kept free of the `vscode` API so the fallbacks are unit-testable; the server's
 * `nullscan_core::RULES` registry stays the single source of truth.
 */

/** One rule as the language server serializes it. */
export interface RuleInfo {
  id: string;
  title: string;
  description: string;
  why: string;
  severity: string;
  docs_url: string;
}

/** Where every rule's anchor lives; used when the catalog is unavailable. */
export const DOCS_BASE_URL = "https://github.com/bytezer/nullscan/blob/main/RULES.md";

/** Indexes the catalog by rule id; later entries win, order is preserved. */
export function indexRules(rules: readonly RuleInfo[]): Map<string, RuleInfo> {
  const index = new Map<string, RuleInfo>();
  for (const rule of rules) {
    index.set(rule.id, rule);
  }
  return index;
}

/** The human rule name, or the id when the catalog has no entry. */
export function titleOf(rules: ReadonlyMap<string, RuleInfo>, id: string): string {
  return nonEmpty(rules.get(id)?.title) ?? id;
}

/** The one-sentence rationale, or an empty string when the catalog has no entry. */
export function whyOf(rules: ReadonlyMap<string, RuleInfo>, id: string): string {
  return nonEmpty(rules.get(id)?.why) ?? "";
}

/** The rule's `RULES.md` anchor, or the generic page when it is unknown. */
export function docsUrlOf(rules: ReadonlyMap<string, RuleInfo>, id: string): string {
  return nonEmpty(rules.get(id)?.docs_url) ?? DOCS_BASE_URL;
}

/**
 * Reads a rule id from a command argument: a plain string from the palette or a
 * tree node such as `{ kind: "rule", rule: "no-console" }`.
 */
export function ruleIdOfArgument(argument: unknown): string | undefined {
  if (typeof argument === "string") {
    return nonEmpty(argument);
  }
  if (typeof argument === "object" && argument !== null) {
    const rule = (argument as { rule?: unknown }).rule;
    return typeof rule === "string" ? nonEmpty(rule) : undefined;
  }
  return undefined;
}

function nonEmpty(value: string | undefined): string | undefined {
  const trimmed = value?.trim();
  return trimmed !== undefined && trimmed.length > 0 ? value : undefined;
}
