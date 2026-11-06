import * as vscode from "vscode";
import type { LanguageClient } from "vscode-languageclient/node";

import {
  groupByFile,
  severityLevelOf,
  severityTotals,
  type Issue,
  type SeverityLevel,
  type SeverityTotals,
} from "./aggregate";

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

export function issueCount(summary: SummaryPayload): number {
  const severity = summary.bySeverity;
  return severity.error + severity.warning + severity.info;
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

/** Asks the server for the cached summary; never triggers a scan. */
export async function requestSummary(client: LanguageClient): Promise<SummaryResponse> {
  return client.sendRequest<SummaryResponse>("nullscan/summary");
}

/**
 * Status bar item that shows the diagnostics VS Code currently holds and falls
 * back to the cached scan summary before the first publication.
 */
export class StatusBar implements vscode.Disposable {
  private readonly item: vscode.StatusBarItem;
  private totals: SeverityTotals | undefined;
  private filesWithIssues = 0;
  private scan: SummaryPayload | null | undefined;

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

  /** Workspace-scan numbers from the server; complements the diagnostics snapshot. */
  setSummary(summary: SummaryPayload | null): void {
    this.scan = summary;
    this.render();
  }

  /** Aggregated diagnostics; keeps the bar correct as open buffers change. */
  setIssues(issues: readonly Issue[]): void {
    this.totals = severityTotals(issues);
    this.filesWithIssues = groupByFile(issues).length;
    this.render();
  }

  setUnavailable(message: string): void {
    this.item.text = "$(shield) Nullscan: server unavailable";
    this.item.tooltip = message;
    this.item.backgroundColor = new vscode.ThemeColor("statusBarItem.errorBackground");
  }

  dispose(): void {
    this.item.dispose();
  }

  private render(): void {
    const totals = this.totals;
    if (totals === undefined) {
      this.renderScan();
      return;
    }
    const total = totals.error + totals.warning + totals.info;
    this.item.text = `$(shield) Nullscan: ${formatIssueCount(total)}`;
    this.item.tooltip = this.issueTooltip(totals, total);
    this.item.backgroundColor = backgroundFor(severityLevelOf(totals));
  }

  private renderScan(): void {
    const summary = this.scan;
    if (summary === undefined || summary === null) {
      this.item.text = "$(shield) Nullscan";
      this.item.tooltip = "Nullscan: no workspace scan has run yet.";
      this.item.backgroundColor = undefined;
      return;
    }
    this.item.text = `$(shield) Nullscan: ${formatIssueCount(issueCount(summary))}`;
    this.item.tooltip = summaryTooltip(summary);
    this.item.backgroundColor = backgroundFor(severityLevelOf(summary.bySeverity));
  }

  private issueTooltip(totals: SeverityTotals, total: number): vscode.MarkdownString {
    const tooltip = new vscode.MarkdownString();
    tooltip.appendMarkdown("**Nullscan**\n\n");
    tooltip.appendMarkdown(
      `${formatIssueCount(total)} in ${formatFileCount(this.filesWithIssues)}: ${totals.error} errors, ${totals.warning} warnings, ${totals.info} infos\n\n`,
    );
    const scan = this.scan;
    if (scan !== undefined && scan !== null) {
      tooltip.appendMarkdown(
        `${scan.filesScanned} files scanned, ${formatDuplicateCount(scan.duplicates)}, ${formatDuration(scan.durationMs)}\n\n`,
      );
      if (scan.truncated) {
        tooltip.appendMarkdown("Results truncated by `nullscan.maxDiagnostics`.\n\n");
      }
    }
    tooltip.appendMarkdown("Click to show the summary.");
    return tooltip;
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

function formatFileCount(count: number): string {
  return count === 1 ? "1 file" : `${count} files`;
}

export function formatDuration(durationMs: number): string {
  return `${(durationMs / 1000).toFixed(1)} s`;
}
