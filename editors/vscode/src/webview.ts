import { randomBytes } from "node:crypto";

import * as vscode from "vscode";

import {
  groupByFile,
  groupByRule,
  severityTotals,
  type FileGroup,
  type Issue,
  type RuleGroup,
} from "./aggregate";
import { duplicationCard, type DuplicationStats } from "./duplication";
import { titleOf, type RuleInfo } from "./rules";
import { formatDuration, type SummaryPayload } from "./summary";

export interface SummaryViewState {
  issues: readonly Issue[];
  summary: SummaryPayload | null;
  stale: boolean;
  configPath: string;
  rules: ReadonlyMap<string, RuleInfo>;
}

/** The two buttons live in the webview; the extension owns what they do. */
export interface SummaryPanelHandlers {
  rescan: () => Promise<void>;
  openConfig: () => Promise<void>;
}

const TOP_LIMIT = 10;
const PANEL_VIEW_TYPE = "nullscan.summary";

/** Single summary webview; `show` reveals the existing panel instead of creating another. */
export class SummaryPanel implements vscode.Disposable {
  private panel: vscode.WebviewPanel | undefined;
  private panelDisposables: vscode.Disposable[] = [];

  constructor(private readonly handlers: SummaryPanelHandlers) {}

  show(state: SummaryViewState): void {
    this.ensurePanel();
    this.update(state);
    this.panel?.reveal(undefined, false);
  }

  /** Re-renders only when the panel exists; diagnostics changes call this often. */
  update(state: SummaryViewState): void {
    const panel = this.panel;
    if (panel === undefined) {
      return;
    }
    panel.webview.html = renderSummaryHtml(state, createNonce());
  }

  dispose(): void {
    const panel = this.panel;
    this.destroyPanel();
    panel?.dispose();
  }

  private ensurePanel(): vscode.WebviewPanel {
    const existing = this.panel;
    if (existing !== undefined) {
      return existing;
    }
    const panel = vscode.window.createWebviewPanel(
      PANEL_VIEW_TYPE,
      "Nullscan summary",
      vscode.ViewColumn.Active,
      { enableScripts: true, retainContextWhenHidden: false, localResourceRoots: [] },
    );
    this.panel = panel;
    this.panelDisposables = [
      panel.onDidDispose(() => this.destroyPanel()),
      panel.webview.onDidReceiveMessage((message: unknown) => {
        void this.onMessage(message);
      }),
    ];
    return panel;
  }

  private destroyPanel(): void {
    this.panel = undefined;
    for (const disposable of this.panelDisposables) {
      disposable.dispose();
    }
    this.panelDisposables = [];
  }

  private async onMessage(message: unknown): Promise<void> {
    if (typeof message !== "object" || message === null) {
      return;
    }
    const type = (message as { type?: unknown }).type;
    if (type === "rescan") {
      await this.handlers.rescan();
    } else if (type === "openConfig") {
      await this.handlers.openConfig();
    }
  }
}

/** The nearest `nullscan.toml` the server will read, or `(defaults)` without a folder. */
export function resolveConfigPath(): string {
  const folder = vscode.workspace.workspaceFolders?.[0];
  if (folder === undefined) {
    return "(defaults)";
  }
  return vscode.Uri.joinPath(folder.uri, "nullscan.toml").fsPath;
}

/** Full document for the panel: one inline `<style>` and one nonce-guarded `<script>`. */
export function renderSummaryHtml(state: SummaryViewState, nonce: string): string {
  const totals = severityTotals(state.issues);
  const files = groupByFile(state.issues);
  const topRuleGroups = groupByRule(state.issues).slice(0, TOP_LIMIT);
  const total = totals.error + totals.warning + totals.info;
  const stale = state.stale && state.summary !== null;
  return `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; script-src 'nonce-${nonce}';">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Nullscan summary</title>
${STYLES}
</head>
<body>
<header>
<h1>Nullscan summary</h1>
${stale ? '<span class="badge">stale</span>' : ""}
</header>
<main>
<section class="cards">
${card("Errors", String(totals.error), "error")}
${card("Warnings", String(totals.warning), "warning")}
${card("Infos", String(totals.info), "info")}
${card("Total issues", String(total), "total")}
${card("Files with issues", String(files.length), "total")}
</section>
${scanSection(state.summary)}
${duplicationSection(state.summary)}
<section>
<h2>Top rules</h2>
${ruleTable(topRuleGroups, state.rules)}
</section>
<section>
<h2>Hot files</h2>
${fileTable(files.slice(0, TOP_LIMIT))}
</section>
<section>
<h2>Config</h2>
<p class="mono">${escapeHtml(state.configPath)}</p>
</section>
</main>
<footer>
<button data-action="rescan">Rescan</button>
<button data-action="openConfig">Open config</button>
</footer>
<script nonce="${nonce}">
const vscode = acquireVsCodeApi();
for (const button of document.querySelectorAll("[data-action]")) {
  button.addEventListener("click", () => vscode.postMessage({ type: button.dataset.action }));
}
</script>
</body>
</html>`;
}

const STYLES = `<style>
:root { color-scheme: light dark; }
body {
  margin: 0;
  padding: 0 20px 24px;
  font-family: var(--vscode-font-family);
  font-size: var(--vscode-font-size);
  color: var(--vscode-foreground);
  background: var(--vscode-editor-background);
}
header { display: flex; align-items: center; gap: 10px; padding-top: 18px; }
h1 { margin: 0; font-size: 1.4em; font-weight: 600; }
h2 { margin: 24px 0 8px; font-size: 1.05em; font-weight: 600; }
.badge {
  padding: 1px 8px;
  border: 1px solid var(--vscode-editorWarning-foreground);
  border-radius: 10px;
  color: var(--vscode-editorWarning-foreground);
  font-size: 0.75em;
  text-transform: uppercase;
  letter-spacing: 0.04em;
}
.cards {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(120px, 1fr));
  gap: 10px;
  margin-top: 18px;
}
.card {
  padding: 12px;
  border: 1px solid var(--vscode-panel-border);
  border-radius: 6px;
  background: var(--vscode-sideBar-background);
}
.card .value { display: block; font-size: 1.6em; font-weight: 600; }
.card .label { display: block; opacity: 0.8; font-size: 0.85em; }
.card.error .value { color: var(--vscode-errorForeground); }
.card.warning .value { color: var(--vscode-editorWarning-foreground); }
.card.info .value { color: var(--vscode-editorInfo-foreground); }
table { width: 100%; border-collapse: collapse; }
th, td { padding: 5px 8px; text-align: left; vertical-align: top; border-bottom: 1px solid var(--vscode-panel-border); }
th { font-weight: 600; opacity: 0.8; }
.num { text-align: right; }
.mono { margin: 0; font-family: var(--vscode-editor-font-family); overflow-wrap: anywhere; }
.rule-title { display: block; font-weight: 600; }
.rule-id { display: block; opacity: 0.7; font-size: 0.85em; }
.sev { font-size: 0.85em; text-transform: uppercase; }
.sev-error { color: var(--vscode-errorForeground); }
.sev-warning { color: var(--vscode-editorWarning-foreground); }
.sev-info { color: var(--vscode-editorInfo-foreground); }
.scan { display: grid; grid-template-columns: max-content 1fr; gap: 4px 16px; margin: 0; }
.scan dt { opacity: 0.8; }
.scan dd { margin: 0; }
.duplication-head { display: flex; align-items: baseline; gap: 8px; }
.duplication-head .value { font-size: 1.6em; font-weight: 600; }
.duplication-head .label { font-size: 0.85em; opacity: 0.8; }
.duplication .bar {
  height: 8px;
  margin: 10px 0 12px;
  border-radius: 4px;
  background: var(--vscode-panel-border);
  overflow: hidden;
}
.duplication .bar-fill { height: 100%; background: var(--vscode-progressBar-background); }
.empty { opacity: 0.7; }
.warning { color: var(--vscode-editorWarning-foreground); }
footer {
  position: sticky;
  bottom: 0;
  display: flex;
  gap: 8px;
  padding: 14px 0 0;
  background: var(--vscode-editor-background);
}
button {
  padding: 4px 14px;
  border: none;
  border-radius: 2px;
  color: var(--vscode-button-foreground);
  background: var(--vscode-button-background);
  font-family: inherit;
  font-size: inherit;
  cursor: pointer;
}
button:hover { background: var(--vscode-button-hoverBackground); }
</style>`;

function card(label: string, value: string, className: string): string {
  return `<div class="card ${className}"><span class="value">${value}</span><span class="label">${escapeHtml(label)}</span></div>`;
}

function scanSection(summary: SummaryPayload | null): string {
  if (summary === null) {
    return `<section>
<h2>Last scan</h2>
<p class="empty">No workspace scan has run yet. Use <strong>Rescan</strong> to analyze the workspace.</p>
</section>`;
  }
  const truncated = summary.truncated
    ? '<p class="warning">Results were truncated by <code>nullscan.maxDiagnostics</code>.</p>'
    : "";
  return `<section>
<h2>Last scan</h2>
<dl class="scan">
<dt>Files scanned</dt><dd>${summary.filesScanned}</dd>
<dt>Duration</dt><dd>${formatDuration(summary.durationMs)}</dd>
<dt>Duplicates</dt><dd>${summary.duplicates}</dd>
<dt>Truncated</dt><dd>${summary.truncated ? "yes" : "no"}</dd>
</dl>
${truncated}
</section>`;
}

/** Line-density card: one percentage, its bar, and the counts behind it. */
function duplicationSection(summary: SummaryPayload | null): string {
  if (summary === null) {
    return emptyDuplication(
      "No workspace scan has run yet. Use <strong>Rescan</strong> to analyze the workspace.",
    );
  }
  // An older server may answer without the metric, so keep the panel usable.
  const stats: DuplicationStats | undefined = summary.duplication;
  if (stats === undefined) {
    return emptyDuplication("This server does not report the duplication metric.");
  }
  const card = duplicationCard(stats);
  return `<section>
<h2>Duplication</h2>
<div class="card duplication">
<div class="duplication-head"><span class="value">${card.density}</span><span class="label">of lines of code are duplicated</span></div>
<div class="bar" role="img" aria-label="Duplicated lines: ${card.density}"><div class="bar-fill" style="width: ${card.barWidth}"></div></div>
<dl class="scan">
<dt>Lines</dt><dd>${card.lines}</dd>
<dt>Blocks</dt><dd>${card.blocks}</dd>
<dt>Files</dt><dd>${card.files}</dd>
<dt>Largest block</dt><dd>${card.largestBlock}</dd>
</dl>
</div>
</section>`;
}

function emptyDuplication(message: string): string {
  return `<section>
<h2>Duplication</h2>
<p class="empty">${message}</p>
</section>`;
}

function ruleTable(groups: readonly RuleGroup[], rules: ReadonlyMap<string, RuleInfo>): string {
  if (groups.length === 0) {
    return '<p class="empty">No issues found.</p>';
  }
  const rows = groups
    .map(
      (group) => `<tr>
<td>${ruleLabel(group.rule, rules)}</td>
<td><span class="sev sev-${group.severity}">${group.severity}</span></td>
<td class="num">${group.count}</td>
</tr>`,
    )
    .join("\n");
  return `<table>
<thead><tr><th>Rule</th><th>Severity</th><th class="num">Issues</th></tr></thead>
<tbody>
${rows}
</tbody>
</table>`;
}

/** Human title first, rule id underneath; the id alone until the catalog answers. */
function ruleLabel(rule: string, rules: ReadonlyMap<string, RuleInfo>): string {
  const title = titleOf(rules, rule);
  if (title === rule) {
    return `<span class="mono">${escapeHtml(title)}</span>`;
  }
  return `<span class="rule-title">${escapeHtml(title)}</span><span class="rule-id mono">${escapeHtml(rule)}</span>`;
}

function fileTable(groups: readonly FileGroup[]): string {
  if (groups.length === 0) {
    return '<p class="empty">No files with issues.</p>';
  }
  const rows = groups
    .map(
      (group) => `<tr>
<td class="mono">${escapeHtml(displayPath(group.file))}</td>
<td class="num">${group.count}</td>
</tr>`,
    )
    .join("\n");
  return `<table>
<thead><tr><th>File</th><th class="num">Issues</th></tr></thead>
<tbody>
${rows}
</tbody>
</table>`;
}

function displayPath(file: string): string {
  return vscode.workspace.asRelativePath(file, false);
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

function createNonce(): string {
  return randomBytes(16).toString("base64");
}
