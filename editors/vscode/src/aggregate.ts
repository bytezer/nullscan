import { fileURLToPath } from "node:url";

/** Severity levels nullscan reports, plus `clean` for an empty set of issues. */
export type SeverityLevel = "error" | "warning" | "info" | "clean";

/** Severity levels a single issue can carry. */
export type IssueSeverity = Exclude<SeverityLevel, "clean">;

/** One published nullscan diagnostic, with 1-based positions for display. */
export interface Issue {
  uri: string;
  file: string;
  line: number;
  column: number;
  endLine: number;
  endColumn: number;
  rule: string;
  severity: IssueSeverity;
  message: string;
}

export interface SeverityTotals {
  error: number;
  warning: number;
  info: number;
}

/**
 * Structural copy of the VS Code `Diagnostic` fields used here, so this module
 * stays free of the `vscode` API and can be unit-tested.
 */
export interface DiagnosticLike {
  range: {
    start: { line: number; character: number };
    end: { line: number; character: number };
  };
  message: string;
  severity?: number | undefined;
  code?: string | number | { value: string | number } | undefined;
}

export interface RuleGroup {
  rule: string;
  count: number;
  severity: IssueSeverity;
  issues: Issue[];
}

export interface FileGroup {
  file: string;
  count: number;
  severity: IssueSeverity;
  issues: Issue[];
}

const UNKNOWN_RULE = "unknown";

/** Maps `[uri, diagnostics]` pairs into issues, preserving their input order. */
export function toIssues(
  diagnosticsByUri: Iterable<readonly [string, readonly DiagnosticLike[]]>,
): Issue[] {
  const issues: Issue[] = [];
  for (const [uri, diagnostics] of diagnosticsByUri) {
    const file = fileFromUri(uri);
    for (const diagnostic of diagnostics) {
      issues.push(toIssue(uri, file, diagnostic));
    }
  }
  return issues;
}

/** Groups issues by rule; count descending, then rule id ascending. */
export function groupByRule(issues: readonly Issue[]): RuleGroup[] {
  const members = new Map<string, Issue[]>();
  for (const issue of issues) {
    push(members, issue.rule, issue);
  }
  const groups: RuleGroup[] = [];
  for (const [rule, ruleIssues] of members) {
    groups.push({
      rule,
      count: ruleIssues.length,
      severity: worstIssueSeverity(ruleIssues),
      issues: ruleIssues,
    });
  }
  return groups.sort((a, b) => compareCountThenText(a.count, a.rule, b.count, b.rule));
}

/** Groups issues by file path; count descending, then file path ascending. */
export function groupByFile(issues: readonly Issue[]): FileGroup[] {
  const members = new Map<string, Issue[]>();
  for (const issue of issues) {
    push(members, issue.file, issue);
  }
  const groups: FileGroup[] = [];
  for (const [file, fileIssues] of members) {
    groups.push({
      file,
      count: fileIssues.length,
      severity: worstIssueSeverity(fileIssues),
      issues: fileIssues,
    });
  }
  return groups.sort((a, b) => compareCountThenText(a.count, a.file, b.count, b.file));
}

export function severityTotals(issues: readonly Issue[]): SeverityTotals {
  const totals: SeverityTotals = { error: 0, warning: 0, info: 0 };
  for (const issue of issues) {
    totals[issue.severity] += 1;
  }
  return totals;
}

/** Highest severity present in a set of issues; `clean` when there are none. */
export function worstSeverity(issues: readonly Issue[]): SeverityLevel {
  return severityLevelOf(severityTotals(issues));
}

/** Highest severity present in precomputed totals; `clean` when all are zero. */
export function severityLevelOf(totals: SeverityTotals): SeverityLevel {
  if (totals.error > 0) {
    return "error";
  }
  if (totals.warning > 0) {
    return "warning";
  }
  if (totals.info > 0) {
    return "info";
  }
  return "clean";
}

/** Worst severity of a non-empty group; groups always carry at least one issue. */
function worstIssueSeverity(issues: readonly Issue[]): IssueSeverity {
  const totals = severityTotals(issues);
  if (totals.error > 0) {
    return "error";
  }
  return totals.warning > 0 ? "warning" : "info";
}

/** The `n` rules with the most issues, reusing `groupByRule` order. */
export function topRules(issues: readonly Issue[], n: number): RuleGroup[] {
  return groupByRule(issues).slice(0, limit(n));
}

/** The `n` files with the most issues, reusing `groupByFile` order. */
export function hotFiles(issues: readonly Issue[], n: number): FileGroup[] {
  return groupByFile(issues).slice(0, limit(n));
}

/**
 * Maps VS Code's `DiagnosticSeverity` numbers: 0 error, 1 warning, 2 information,
 * 3 hint. Hints and unknown values are reported as `info`.
 */
export function severityOf(severity: number | undefined): IssueSeverity {
  switch (severity) {
    case 0:
      return "error";
    case 1:
      return "warning";
    default:
      return "info";
  }
}

function toIssue(uri: string, file: string, diagnostic: DiagnosticLike): Issue {
  const { range } = diagnostic;
  return {
    uri,
    file,
    line: range.start.line + 1,
    column: range.start.character + 1,
    endLine: range.end.line + 1,
    endColumn: range.end.character + 1,
    rule: ruleOf(diagnostic),
    severity: severityOf(diagnostic.severity),
    message: diagnostic.message,
  };
}

function ruleOf(diagnostic: DiagnosticLike): string {
  const code = diagnostic.code;
  if (typeof code === "string" && code.length > 0) {
    return code;
  }
  if (typeof code === "number") {
    return String(code);
  }
  if (code !== undefined && typeof code === "object") {
    const value = code.value;
    if (typeof value === "string" && value.length > 0) {
      return value;
    }
    if (typeof value === "number") {
      return String(value);
    }
  }
  return UNKNOWN_RULE;
}

function fileFromUri(uri: string): string {
  if (!uri.startsWith("file:")) {
    return uri;
  }
  try {
    return fileURLToPath(uri);
  } catch {
    return uri;
  }
}

function push(groups: Map<string, Issue[]>, key: string, issue: Issue): void {
  const existing = groups.get(key);
  if (existing === undefined) {
    groups.set(key, [issue]);
    return;
  }
  existing.push(issue);
}

function limit(n: number): number {
  return Number.isFinite(n) ? Math.max(0, Math.trunc(n)) : 0;
}

function compareCountThenText(
  countA: number,
  textA: string,
  countB: number,
  textB: string,
): number {
  if (countA !== countB) {
    return countB - countA;
  }
  if (textA === textB) {
    return 0;
  }
  return textA < textB ? -1 : 1;
}
