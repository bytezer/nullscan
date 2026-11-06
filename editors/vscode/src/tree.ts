import * as vscode from "vscode";

import {
  groupByFile,
  groupByRule,
  toIssues,
  type Issue,
  type IssueSeverity,
} from "./aggregate";
import { formatIssueCount } from "./summary";

export const VIEW_BY_RULE = "nullscan.byRule";
export const VIEW_BY_FILE = "nullscan.byFile";
export const OPEN_ISSUE_COMMAND = "nullscan.openIssue";

/** Diagnostics can arrive in bursts (a workspace scan publishes per file). */
const REFRESH_DEBOUNCE_MS = 250;

type TreeMode = "rule" | "file";

type IssueNode = RuleNode | FileNode | DiagnosticNode;

interface RuleNode {
  kind: "rule";
  rule: string;
  count: number;
  severity: IssueSeverity;
}

interface FileNode {
  kind: "file";
  file: string;
  count: number;
}

interface DiagnosticNode {
  kind: "issue";
  issue: Issue;
  context: TreeMode;
}

const SEVERITY_ICONS: Readonly<Record<IssueSeverity, vscode.ThemeIcon>> = {
  error: new vscode.ThemeIcon("error"),
  warning: new vscode.ThemeIcon("warning"),
  info: new vscode.ThemeIcon("info"),
};

/**
 * Reads the diagnostics VS Code already holds and keeps the two trees and the
 * status bar in sync. It never asks the server for issues.
 */
export class IssuesController implements vscode.Disposable {
  private readonly byRuleProvider = new IssuesProvider("rule");
  private readonly byFileProvider = new IssuesProvider("file");
  private readonly views: Array<vscode.TreeView<IssueNode>>;
  private readonly diagnosticsListener: vscode.Disposable;
  private readonly openIssueCommand: vscode.Disposable;
  private issues: readonly Issue[] = [];
  private timer: ReturnType<typeof setTimeout> | undefined;

  constructor(private readonly onIssues: (issues: readonly Issue[]) => void) {
    this.diagnosticsListener = vscode.languages.onDidChangeDiagnostics(() => {
      this.scheduleRefresh();
    });
    this.openIssueCommand = registerOpenIssueCommand();
    this.views = [
      vscode.window.createTreeView(VIEW_BY_RULE, {
        treeDataProvider: this.byRuleProvider,
        showCollapseAll: true,
      }),
      vscode.window.createTreeView(VIEW_BY_FILE, {
        treeDataProvider: this.byFileProvider,
        showCollapseAll: true,
      }),
    ];
    this.refresh();
  }

  /** Latest snapshot, shared with the status bar and the summary panel. */
  current(): readonly Issue[] {
    return this.issues;
  }

  /** Re-reads diagnostics now; used by the refresh command and after scans. */
  refresh(): void {
    this.issues = collectIssues();
    this.byRuleProvider.setIssues(this.issues);
    this.byFileProvider.setIssues(this.issues);
    this.onIssues(this.issues);
  }

  private scheduleRefresh(): void {
    if (this.timer !== undefined) {
      clearTimeout(this.timer);
    }
    this.timer = setTimeout(() => {
      this.timer = undefined;
      this.refresh();
    }, REFRESH_DEBOUNCE_MS);
  }

  dispose(): void {
    if (this.timer !== undefined) {
      clearTimeout(this.timer);
      this.timer = undefined;
    }
    this.diagnosticsListener.dispose();
    this.openIssueCommand.dispose();
    for (const view of this.views) {
      view.dispose();
    }
    this.byRuleProvider.dispose();
    this.byFileProvider.dispose();
  }
}

/** `languages.getDiagnostics()` filtered to nullscan's own publications. */
export function collectIssues(): Issue[] {
  const entries = vscode.languages
    .getDiagnostics()
    .map(([uri, diagnostics]) => [uri.toString(), diagnostics.filter(isNullscan)] as const);
  return toIssues(entries);
}

/** Opens the issue's document, selects its range and reveals it. */
export function registerOpenIssueCommand(): vscode.Disposable {
  return vscode.commands.registerCommand(OPEN_ISSUE_COMMAND, (issue: Issue) => openIssue(issue));
}

class IssuesProvider implements vscode.TreeDataProvider<IssueNode> {
  private readonly changed = new vscode.EventEmitter<void>();
  readonly onDidChangeTreeData = this.changed.event;
  private issues: readonly Issue[] = [];

  constructor(private readonly mode: TreeMode) {}

  setIssues(issues: readonly Issue[]): void {
    this.issues = issues;
    this.changed.fire();
  }

  getTreeItem(node: IssueNode): vscode.TreeItem {
    switch (node.kind) {
      case "rule":
        return ruleItem(node);
      case "file":
        return fileItem(node);
      case "issue":
        return issueItem(node);
    }
  }

  getChildren(node?: IssueNode): IssueNode[] {
    if (node === undefined) {
      return this.roots();
    }
    switch (node.kind) {
      case "rule":
        return childrenOfRule(this.issues, node.rule);
      case "file":
        return childrenOfFile(this.issues, node.file);
      case "issue":
        return [];
    }
  }

  dispose(): void {
    this.changed.dispose();
  }

  private roots(): IssueNode[] {
    if (this.mode === "rule") {
      return groupByRule(this.issues).map((group) => ({
        kind: "rule",
        rule: group.rule,
        count: group.count,
        severity: group.severity,
      }));
    }
    return groupByFile(this.issues).map((group) => ({
      kind: "file",
      file: group.file,
      count: group.count,
    }));
  }
}

function childrenOfRule(issues: readonly Issue[], rule: string): IssueNode[] {
  return issues
    .filter((issue) => issue.rule === rule)
    .map((issue) => ({ kind: "issue", issue, context: "rule" }));
}

function childrenOfFile(issues: readonly Issue[], file: string): IssueNode[] {
  return issues
    .filter((issue) => issue.file === file)
    .map((issue) => ({ kind: "issue", issue, context: "file" }));
}

function ruleItem(node: RuleNode): vscode.TreeItem {
  const item = new vscode.TreeItem(node.rule, vscode.TreeItemCollapsibleState.Collapsed);
  item.id = `rule:${node.rule}`;
  item.description = String(node.count);
  item.iconPath = SEVERITY_ICONS[node.severity];
  item.tooltip = formatIssueCount(node.count);
  item.contextValue = "nullscan.ruleGroup";
  return item;
}

function fileItem(node: FileNode): vscode.TreeItem {
  const item = new vscode.TreeItem(
    displayPath(node.file),
    vscode.TreeItemCollapsibleState.Collapsed,
  );
  item.id = `file:${node.file}`;
  item.description = String(node.count);
  item.iconPath = new vscode.ThemeIcon("file-code");
  item.tooltip = `${formatIssueCount(node.count)} in ${node.file}`;
  item.contextValue = "nullscan.fileGroup";
  return item;
}

function issueItem(node: DiagnosticNode): vscode.TreeItem {
  const { issue } = node;
  const item = new vscode.TreeItem(issue.message, vscode.TreeItemCollapsibleState.None);
  item.id = `issue:${issue.uri}:${issue.line}:${issue.column}:${issue.rule}`;
  item.description =
    node.context === "rule"
      ? `${displayPath(issue.file)}:${issue.line}`
      : `${issue.line}:${issue.column}`;
  item.iconPath = SEVERITY_ICONS[issue.severity];
  item.tooltip = `${issue.rule} (${issue.severity}): ${issue.message}`;
  item.contextValue = "nullscan.issue";
  item.command = {
    command: OPEN_ISSUE_COMMAND,
    title: "Open Issue",
    arguments: [issue],
  };
  return item;
}

function displayPath(file: string): string {
  return vscode.workspace.asRelativePath(file, false);
}

async function openIssue(issue: Issue): Promise<void> {
  const document = await vscode.workspace.openTextDocument(vscode.Uri.parse(issue.uri));
  const editor = await vscode.window.showTextDocument(document, { preview: false });
  const range = new vscode.Range(
    issue.line - 1,
    issue.column - 1,
    issue.endLine - 1,
    issue.endColumn - 1,
  );
  editor.selection = new vscode.Selection(range.start, range.end);
  editor.revealRange(range, vscode.TextEditorRevealType.InCenterIfOutsideViewport);
}

function isNullscan(diagnostic: vscode.Diagnostic): boolean {
  return diagnostic.source === "nullscan";
}
