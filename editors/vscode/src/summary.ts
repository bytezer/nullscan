import * as vscode from "vscode";
import type { LanguageClient } from "vscode-languageclient/node";

/** The `summary` object shared by `--format json` and both custom LSP requests. */
export interface SummaryPayload {
  filesScanned: number;
  filesWithIssues: number;
  durationMs: number;
  bySeverity: { error: number; warning: number; info: number };
  byRule: Array<{ rule: string; count: number }>;
  duplicates: number;
  truncated: boolean;
}

/** `nullscan/summary` caches the last scan; `stale` means buffers changed since then. */
export interface SummaryResponse {
  summary: SummaryPayload | null;
  stale: boolean;
}

export type SeverityLevel = "error" | "warning" | "info" | "clean";

export function issueCount(summary: SummaryPayload): number {
  const severity = summary.bySeverity;
  return severity.error + severity.warning + severity.info;
}

export function highestSeverity(summary: SummaryPayload): SeverityLevel {
  if (summary.bySeverity.error > 0) {
    return "error";
  }
  if (summary.bySeverity.warning > 0) {
    return "warning";
  }
  if (summary.bySeverity.info > 0) {
    return "info";
  }
  return "clean";
}

export function formatIssueCount(count: number): string {
  return count === 1 ? "1 issue" : `${count} issues`;
}

export function formatScanResult(summary: SummaryPayload): string {
  const duplicates =
    summary.duplicates === 1 ? "1 duplicate" : `${summary.duplicates} duplicates`;
  return `Nullscan: ${summary.filesScanned} files, ${formatIssueCount(
    issueCount(summary),
  )}, ${duplicates} (${formatDuration(summary.durationMs)}).`;
}

export function describeSummary(summary: SummaryPayload, stale: boolean): string {
  const severity = summary.bySeverity;
  const lines = [
    `Files scanned: ${summary.filesScanned} (${summary.filesWithIssues} with issues)`,
    `Issues: ${issueCount(summary)} (${severity.error} errors, ${severity.warning} warnings, ${severity.info} infos)`,
    `Duplicates: ${summary.duplicates}`,
    `Duration: ${formatDuration(summary.durationMs)}`,
  ];
  if (summary.truncated) {
    lines.push("Results were truncated by nullscan.maxDiagnostics.");
  }
  if (stale) {
    lines.push("The workspace changed since this scan; run it again for fresh numbers.");
  }
  return lines.join("\n");
}

/** Asks the server for the cached summary; never triggers a scan. */
export async function requestSummary(client: LanguageClient): Promise<SummaryResponse> {
  return client.sendRequest<SummaryResponse>("nullscan/summary");
}

/** Reads the cached summary and mirrors it in the status bar. */
export async function refreshStatusBar(
  client: LanguageClient,
  statusBar: StatusBar,
): Promise<void> {
  const response = await requestSummary(client);
  statusBar.setSummary(response.summary);
}

/** Status bar item that shows the last summary and opens `nullscan.showSummary` on click. */
export class StatusBar implements vscode.Disposable {
  private readonly item: vscode.StatusBarItem;

  constructor() {
    this.item = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 100);
    this.item.name = "Nullscan";
    this.item.command = "nullscan.showSummary";
    this.item.text = "$(shield) Nullscan";
    this.item.tooltip = "Nullscan: no workspace scan has run yet.";
    this.item.show();
  }

  setVisible(visible: boolean): void {
    if (visible) {
      this.item.show();
    } else {
      this.item.hide();
    }
  }

  setSummary(summary: SummaryPayload | null): void {
    if (summary === null) {
      this.item.text = "$(shield) Nullscan";
      this.item.tooltip = "Nullscan: no workspace scan has run yet.";
      this.item.backgroundColor = undefined;
      return;
    }
    this.item.text = `$(shield) Nullscan: ${formatIssueCount(issueCount(summary))}`;
    this.item.tooltip = summaryTooltip(summary);
    this.item.backgroundColor = backgroundFor(highestSeverity(summary));
  }

  setUnavailable(message: string): void {
    this.item.text = "$(shield) Nullscan: server unavailable";
    this.item.tooltip = message;
    this.item.backgroundColor = new vscode.ThemeColor("statusBarItem.errorBackground");
  }

  dispose(): void {
    this.item.dispose();
  }
}

function summaryTooltip(summary: SummaryPayload): vscode.MarkdownString {
  const severity = summary.bySeverity;
  const tooltip = new vscode.MarkdownString();
  tooltip.appendMarkdown("**Nullscan**\n\n");
  tooltip.appendMarkdown(
    `${formatIssueCount(issueCount(summary))}: ${severity.error} errors, ${severity.warning} warnings, ${severity.info} infos\n\n`,
  );
  tooltip.appendMarkdown(
    `${summary.filesScanned} files scanned, ${summary.filesWithIssues} with issues\n\n`,
  );
  tooltip.appendMarkdown(
    `${formatDuplicateCount(summary.duplicates)} found\n\nDuration: ${formatDuration(summary.durationMs)}`,
  );
  if (summary.truncated) {
    tooltip.appendMarkdown("\n\nResults truncated by `nullscan.maxDiagnostics`.");
  }
  tooltip.appendMarkdown("\n\nClick to show the summary.");
  return tooltip;
}

function backgroundFor(level: SeverityLevel): vscode.ThemeColor | undefined {
  switch (level) {
    case "error":
      return new vscode.ThemeColor("statusBarItem.errorBackground");
    case "warning":
      return new vscode.ThemeColor("statusBarItem.warningBackground");
    case "info":
    case "clean":
      return undefined;
  }
}

function formatDuplicateCount(count: number): string {
  return count === 1 ? "1 duplicate" : `${count} duplicates`;
}

function formatDuration(durationMs: number): string {
  return `${(durationMs / 1000).toFixed(1)} s`;
}
