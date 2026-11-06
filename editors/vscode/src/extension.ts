import { execFile } from "node:child_process";

import * as vscode from "vscode";
import {
  LanguageClient,
  type LanguageClientOptions,
  type ServerOptions,
} from "vscode-languageclient/node";

import type { Issue } from "./aggregate";
import {
  parseServerVersion,
  resolveServerBinary,
  serverVersionMatches,
  type BinarySource,
} from "./binary";
import { resolveExplicitConfigPath } from "./configPath";
import {
  cachedServerLocation,
  downloadServer,
  removeCachedServer,
  type DownloadServerResult,
} from "./download";
import {
  DOCS_BASE_URL,
  docsUrlOf,
  indexRules,
  ruleIdOfArgument,
  type RuleInfo,
} from "./rules";
import { readSettings } from "./settings";
import {
  StatusBar,
  formatScanResult,
  requestSummary,
  type SummaryPayload,
  type SummaryResponse,
} from "./summary";
import { IssuesController } from "./tree";
import { SummaryPanel, resolveConfigPath, type SummaryViewState } from "./webview";

const VERSION_TIMEOUT_MS = 5000;
const DOWNLOAD_TIMEOUT_MS = 120_000;

const CONTINUE = "Continue";
const OPEN_SETTINGS = "Open Settings";
const SHOW_OUTPUT = "Show Output";
const RETRY = "Retry";
const RESTART_SERVER = "Restart Server";
const DOWNLOAD_TITLE = "Downloading nullscan server…";
const CREATE_CONFIG = "Create";

let activeExtension: NullscanExtension | undefined;

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  const output = vscode.window.createOutputChannel("Nullscan", { log: true });
  const trace = vscode.window.createOutputChannel("Nullscan (trace)", { log: true });
  const statusBar = new StatusBar();
  const extension = new NullscanExtension(context, output, trace, statusBar);

  context.subscriptions.push(extension, output, trace, statusBar);
  activeExtension = extension;
  await extension.start();
}

export async function deactivate(): Promise<void> {
  const extension = activeExtension;
  activeExtension = undefined;
  await extension?.stop();
}

class NullscanExtension implements vscode.Disposable {
  private client: LanguageClient | undefined;
  private downloadController: AbortController | undefined;
  private disposed = false;
  private readonly disposables: vscode.Disposable[] = [];
  private readonly summaryPanel: SummaryPanel;
  private readonly issues: IssuesController;
  private cachedSummary: SummaryResponse = { summary: null, stale: true };
  private rules: ReadonlyMap<string, RuleInfo> = new Map();

  constructor(
    private readonly context: vscode.ExtensionContext,
    private readonly output: vscode.LogOutputChannel,
    private readonly trace: vscode.LogOutputChannel,
    private readonly statusBar: StatusBar,
  ) {
    this.summaryPanel = new SummaryPanel({
      rescan: () => this.analyzeWorkspace(),
      openConfig: () => this.openConfig(),
    });
    this.issues = new IssuesController((issues) => this.onIssuesChanged(issues));
    this.disposables.push(...this.registerCommands());
    this.disposables.push(
      vscode.workspace.onDidChangeConfiguration((event) => {
        void this.onConfigurationChanged(event);
      }),
      this.summaryPanel,
      this.issues,
    );
  }

  async start(): Promise<void> {
    if (this.disposed) {
      return;
    }
    const settings = readSettings();
    this.statusBar.setVisible(settings.showStatusBar);

    const binary = await this.resolveBinary(settings.path);
    if (binary === undefined || this.disposed) {
      return;
    }

    this.output.appendLine(
      `Nullscan: using ${binary.path} (${sourceLabel(binary.source)}).`,
    );
    await this.checkServerVersion(binary.path);
    if (!this.disposed) {
      await this.startClient(binary.path);
    }
  }

  /**
   * `nullscan.path` → bundled binary → verified download → `PATH`. The download runs
   * only when the setting is empty and no bundled binary exists.
   */
  private async resolveBinary(
    configuredPath: string,
  ): Promise<{ path: string; source: BinarySource } | undefined> {
    if (configuredPath.trim().length > 0) {
      const explicit = resolveServerBinary({
        configuredPath,
        extensionPath: this.context.extensionPath,
        platform: process.platform,
        arch: process.arch,
        includePath: true,
      });
      if ("path" in explicit) {
        return explicit;
      }
      await this.reportMissingBinary(explicit);
      return undefined;
    }

    const version = this.extensionVersion();
    const cached = cachedServerLocation(
      this.cacheDirectory(),
      version,
      process.platform,
      process.arch,
    );
    const local = resolveServerBinary({
      configuredPath: "",
      extensionPath: this.context.extensionPath,
      platform: process.platform,
      arch: process.arch,
      downloadedPath: cached?.path,
      includePath: false,
    });
    if ("path" in local) {
      return local;
    }
    if (cached === undefined) {
      await this.reportMissingBinary(local);
      return undefined;
    }

    const download = await this.downloadBinary(version);
    if ("path" in download) {
      return { path: download.path, source: "download" };
    }

    const onPath = this.findOnPath(configuredPath);
    if (onPath !== undefined) {
      this.output.appendLine(
        `Nullscan: falling back to ${onPath.path} (${sourceLabel(onPath.source)}) because the download did not complete.`,
      );
      return onPath;
    }
    return undefined;
  }

  private async downloadBinary(
    version: string,
  ): Promise<{ path: string } | { cancelled: true } | { failed: true }> {
    const http = vscode.workspace.getConfiguration("http");
    let timedOut = false;
    try {
      const result = await vscode.window.withProgress<DownloadServerResult>(
        {
          location: vscode.ProgressLocation.Notification,
          title: DOWNLOAD_TITLE,
          cancellable: true,
        },
        async (progress, token) => {
          const controller = new AbortController();
          this.downloadController = controller;
          const timer = setTimeout(() => {
            timedOut = true;
            controller.abort();
          }, DOWNLOAD_TIMEOUT_MS);
          const subscription = token.onCancellationRequested(() => controller.abort());
          try {
            return await downloadServer({
              version,
              cacheDir: this.cacheDirectory(),
              signal: controller.signal,
              proxy: http.get<string>("proxy", ""),
              strictSSL: http.get<boolean>("proxyStrictSSL", true),
              log: (line) => this.output.appendLine(`Nullscan: ${line}`),
              onProgress: (downloaded, total) => {
                const progressMessage =
                  total > 0
                    ? `${formatBytes(downloaded)} of ${formatBytes(total)}`
                    : formatBytes(downloaded);
                progress.report({ message: progressMessage });
              },
            });
          } finally {
            clearTimeout(timer);
            subscription.dispose();
            this.downloadController = undefined;
          }
        },
      );
      this.output.appendLine(
        `Nullscan: server ${result.downloaded ? "downloaded" : "reused from the cache"} at ${result.path}.`,
      );
      return { path: result.path };
    } catch (error) {
      if (this.disposed) {
        this.output.appendLine("Nullscan: server download stopped because the extension was disposed.");
        return { cancelled: true };
      }
      if (timedOut) {
        await this.reportDownloadFailure(
          new Error(`The download timed out after ${DOWNLOAD_TIMEOUT_MS / 1000} s.`),
        );
        return { failed: true };
      }
      if (isAbortError(error)) {
        this.output.appendLine("Nullscan: server download cancelled.");
        this.statusBar.setUnavailable("Nullscan: the language server download was cancelled.");
        void vscode.window.showInformationMessage(
          'Nullscan: server download cancelled. Run "Nullscan: Redownload Server" to try again.',
        );
        return { cancelled: true };
      }
      await this.reportDownloadFailure(error);
      return { failed: true };
    }
  }

  private findOnPath(
    configuredPath: string,
  ): { path: string; source: BinarySource } | undefined {
    const fallback = resolveServerBinary({
      configuredPath,
      extensionPath: this.context.extensionPath,
      platform: process.platform,
      arch: process.arch,
      includePath: true,
    });
    if ("path" in fallback) {
      return fallback;
    }
    this.output.appendLine(fallback.error);
    return undefined;
  }

  private async reportDownloadFailure(error: unknown): Promise<void> {
    const message = messageOf(error);
    this.output.appendLine(`Nullscan: server download failed: ${message}`);
    this.output.appendLine(
      "Nullscan: check the http.proxy setting, your network, or set nullscan.path to a local build.",
    );
    this.statusBar.setUnavailable(`Nullscan: server download failed. ${firstLine(message)}`);

    const action = await vscode.window.showErrorMessage(
      `Nullscan: could not download the language server. ${firstLine(message)}`,
      OPEN_SETTINGS,
      SHOW_OUTPUT,
      RETRY,
    );
    if (action === OPEN_SETTINGS) {
      await openNullscanSettings("nullscan.path");
    } else if (action === SHOW_OUTPUT) {
      this.output.show();
    } else if (action === RETRY) {
      await vscode.commands.executeCommand("nullscan.redownloadServer");
    }
  }

  private async redownloadServer(): Promise<void> {
    const version = this.extensionVersion();
    const cached = cachedServerLocation(
      this.cacheDirectory(),
      version,
      process.platform,
      process.arch,
    );
    if (cached === undefined) {
      void vscode.window.showErrorMessage(
        `Nullscan: this platform (${process.platform}-${process.arch}) has no prebuilt language server. Set "nullscan.path" instead.`,
      );
      return;
    }

    this.output.appendLine(
      `Nullscan: deleting ${cached.path} and downloading nullscan-lsp ${version}.`,
    );
    await this.stop();
    try {
      await removeCachedServer(cached);
    } catch (error) {
      this.reportError(`could not delete ${cached.path}`, error);
      return;
    }

    const download = await this.downloadBinary(version);
    if (!("path" in download)) {
      return;
    }
    await this.start();
  }

  private extensionVersion(): string {
    return String(this.context.extension.packageJSON.version ?? "0.0.0");
  }

  private cacheDirectory(): string {
    return this.context.globalStorageUri.fsPath;
  }

  async stop(): Promise<void> {
    const client = this.client;
    this.client = undefined;
    if (client === undefined) {
      return;
    }
    try {
      await client.stop();
      this.output.appendLine("Nullscan: language server stopped.");
    } catch (error) {
      this.output.appendLine(
        `Nullscan: failed to stop the language server: ${messageOf(error)}`,
      );
    }
  }

  dispose(): void {
    this.disposed = true;
    this.downloadController?.abort();
    for (const disposable of this.disposables) {
      disposable.dispose();
    }
    this.disposables.length = 0;
    void this.stop();
  }

  private registerCommands(): vscode.Disposable[] {
    const commands: Array<[string, (...args: unknown[]) => unknown]> = [
      ["nullscan.analyzeWorkspace", () => this.analyzeWorkspace()],
      ["nullscan.showSummary", () => this.showSummary()],
      ["nullscan.openConfig", () => this.openConfig()],
      ["nullscan.showRuleDocumentation", (argument) => this.showRuleDocumentation(argument)],
      ["nullscan.redownloadServer", () => this.redownloadServer()],
      ["nullscan.restartServer", () => this.restartServer()],
      ["nullscan.refreshIssues", () => this.issues.refresh()],
    ];
    return commands.map(([id, run]) => vscode.commands.registerCommand(id, run));
  }

  private async reportMissingBinary(binary: {
    error: string;
    candidates: string[];
  }): Promise<void> {
    this.output.appendLine(binary.error);
    this.output.appendLine(
      `Nullscan: ${binary.candidates.length} candidate paths were checked.`,
    );
    this.statusBar.setUnavailable(binary.error);

    const firstLine = binary.error.split("\n")[0] ?? "nullscan-lsp was not found";
    const action = await vscode.window.showErrorMessage(
      `Nullscan: ${firstLine}`,
      OPEN_SETTINGS,
      SHOW_OUTPUT,
    );
    if (action === OPEN_SETTINGS) {
      await openNullscanSettings("nullscan.path");
    } else if (action === SHOW_OUTPUT) {
      this.output.show();
    }
  }

  private async checkServerVersion(binaryPath: string): Promise<void> {
    const expected = String(this.context.extension.packageJSON.version ?? "");
    let stdout: string;
    try {
      stdout = await runVersion(binaryPath);
    } catch (error) {
      this.output.appendLine(
        `Nullscan: could not run "${binaryPath} --version": ${messageOf(error)}`,
      );
      const action = await vscode.window.showWarningMessage(
        "Nullscan: the language server did not answer --version. Continue only if you know it is the right binary.",
        CONTINUE,
        SHOW_OUTPUT,
      );
      if (action === SHOW_OUTPUT) {
        this.output.show();
      }
      return;
    }

    const actual = parseServerVersion(stdout);
    if (actual === undefined) {
      this.output.appendLine(
        `Nullscan: unexpected --version output: ${JSON.stringify(stdout.trim())}`,
      );
      this.output.appendLine("Nullscan: continuing without a version check.");
      return;
    }

    this.output.appendLine(
      `Nullscan: language server ${actual}, extension ${expected}.`,
    );
    if (serverVersionMatches(expected, actual)) {
      return;
    }

    const action = await vscode.window.showWarningMessage(
      `Nullscan: the language server (${actual}) does not match this extension (${expected}). Update both to the same release.`,
      CONTINUE,
      OPEN_SETTINGS,
    );
    if (action === OPEN_SETTINGS) {
      await openNullscanSettings("nullscan.path");
    }
  }

  /**
   * Server flags in a stable order: `--stdio` first, then `--config <path>` when
   * `nullscan.configPath` is set. A missing file is only a warning; the server falls back
   * to the default rules on its own.
   */
  private async serverArgs(configured: string): Promise<string[]> {
    const resolution = resolveExplicitConfigPath(configured, firstWorkspaceFolder());
    switch (resolution.kind) {
      case "unset":
        return ["--stdio"];
      case "unresolved":
        this.output.appendLine(
          `Nullscan: nullscan.configPath "${resolution.configured}" is relative and there is no workspace folder to resolve it against; the server reads each folder's nullscan.toml.`,
        );
        return ["--stdio"];
      case "resolved":
        this.output.appendLine(`Nullscan: using config ${resolution.path}.`);
        if (!(await fileExists(vscode.Uri.file(resolution.path)))) {
          const message = `Nullscan: the config file ${resolution.path} does not exist. The language server falls back to the default rules.`;
          this.output.appendLine(message);
          void vscode.window.showWarningMessage(message);
        }
        return ["--stdio", "--config", resolution.path];
    }
  }

  private async startClient(binaryPath: string): Promise<void> {
    const settings = readSettings();
    const args = await this.serverArgs(settings.configPath);

    const serverOptions: ServerOptions = {
      command: binaryPath,
      args,
      options: { cwd: firstWorkspaceFolder() },
    };
    const clientOptions: LanguageClientOptions = {
      documentSelector: [
        { scheme: "file", language: "typescript" },
        { scheme: "file", language: "typescriptreact" },
      ],
      synchronize: {
        configurationSection: "nullscan",
        fileEvents: vscode.workspace.createFileSystemWatcher("**/nullscan.toml"),
      },
      initializationOptions: { nullscan: settings },
      outputChannel: this.output,
      traceOutputChannel: this.trace,
    };

    const client = new LanguageClient("nullscan", "Nullscan", serverOptions, clientOptions);
    this.client = client;
    try {
      await client.start();
      this.output.appendLine("Nullscan: language server started.");
    } catch (error) {
      this.client = undefined;
      this.statusBar.setUnavailable(messageOf(error));
      this.reportError("the language server failed to start", error);
      return;
    }

    await this.loadRuleCatalog(client);
    await this.refresh();
  }

  /** `nullscan/rules`; an empty map means titles fall back to the rule ids. */
  private async loadRuleCatalog(client: LanguageClient): Promise<void> {
    try {
      const catalog = await client.sendRequest<RuleInfo[]>("nullscan/rules");
      this.rules = indexRules(Array.isArray(catalog) ? catalog : []);
    } catch (error) {
      this.output.appendLine(
        `Nullscan: could not read the rule catalog: ${messageOf(error)}`,
      );
      this.rules = new Map();
    }
    this.issues.setRules(this.rules);
  }

  private async refresh(): Promise<void> {
    if (this.client === undefined) {
      return;
    }
    try {
      this.cachedSummary = await requestSummary(this.client);
      this.statusBar.setSummary(this.cachedSummary.summary);
      this.summaryPanel.update(this.summaryState(this.issues.current()));
    } catch (error) {
      this.output.appendLine(
        `Nullscan: could not read the cached summary: ${messageOf(error)}`,
      );
    }
  }

  private async analyzeWorkspace(): Promise<void> {
    const client = this.requireClient();
    if (client === undefined) {
      return;
    }

    let cancelled = false;
    try {
      const summary = await vscode.window.withProgress<SummaryPayload>(
        {
          location: vscode.ProgressLocation.Notification,
          title: "Nullscan: analyzing workspace",
          cancellable: true,
        },
        (_progress, token) => {
          token.onCancellationRequested(() => {
            cancelled = true;
          });
          return client.sendRequest<SummaryPayload>("nullscan/analyzeWorkspace", {}, token);
        },
      );
      this.cachedSummary = { summary, stale: false };
      this.statusBar.setSummary(summary);
      this.issues.refresh();
      this.summaryPanel.update(this.summaryState(this.issues.current()));
      void vscode.window.showInformationMessage(formatScanResult(summary));
    } catch (error) {
      if (cancelled) {
        this.output.appendLine("Nullscan: workspace scan cancelled.");
        void vscode.window.showInformationMessage("Nullscan: workspace scan cancelled.");
        return;
      }
      this.reportError("the workspace scan failed", error);
    }
  }

  private async showSummary(): Promise<void> {
    await this.refresh();
    this.summaryPanel.show(this.summaryState(this.issues.current()));
  }

  private onIssuesChanged(issues: readonly Issue[]): void {
    this.statusBar.setIssues(issues);
    this.summaryPanel.update(this.summaryState(issues));
  }

  private summaryState(issues: readonly Issue[]): SummaryViewState {
    return {
      issues,
      summary: this.cachedSummary.summary,
      stale: this.cachedSummary.stale,
      configPath: resolveConfigPath(),
      rules: this.rules,
    };
  }

  /**
   * Opens a rule's `RULES.md` anchor. Tree nodes arrive as their element with a
   * `rule` field; from the palette the catalog drives a QuickPick.
   */
  private async showRuleDocumentation(argument?: unknown): Promise<void> {
    const ruleId = ruleIdOfArgument(argument);
    if (ruleId !== undefined) {
      await openDocumentation(docsUrlOf(this.rules, ruleId));
      return;
    }

    const items = [...this.rules.values()].map((rule) => ({
      label: rule.title,
      description: rule.id,
      url: rule.docs_url,
    }));
    if (items.length === 0) {
      await openDocumentation(DOCS_BASE_URL);
      return;
    }

    const picked = await vscode.window.showQuickPick(items, {
      placeHolder: "Select a rule to open its documentation",
    });
    if (picked !== undefined) {
      await openDocumentation(picked.url);
    }
  }

  private async openConfig(): Promise<void> {
    const folder = await pickWorkspaceFolder();
    if (folder === undefined) {
      void vscode.window.showWarningMessage(
        "Nullscan: open a folder to configure nullscan.toml.",
      );
      return;
    }

    const configUri = vscode.Uri.joinPath(folder.uri, "nullscan.toml");
    if (await fileExists(configUri)) {
      await vscode.window.showTextDocument(configUri, { preview: false });
      return;
    }

    const action = await vscode.window.showInformationMessage(
      `Nullscan: ${folder.name} has no nullscan.toml.`,
      CREATE_CONFIG,
    );
    if (action !== CREATE_CONFIG) {
      return;
    }
    try {
      await vscode.workspace.fs.writeFile(
        configUri,
        Buffer.from("[rules]\ndisabled = []\n", "utf8"),
      );
    } catch (error) {
      this.reportError(`could not create ${configUri.fsPath}`, error);
      return;
    }
    this.output.appendLine(`Nullscan: created ${configUri.fsPath}.`);
    await vscode.window.showTextDocument(configUri, { preview: false });
  }

  private async restartServer(): Promise<void> {
    this.output.appendLine("Nullscan: restarting the language server.");
    await this.stop();
    await this.start();
  }

  private requireClient(): LanguageClient | undefined {
    if (this.client !== undefined) {
      return this.client;
    }
    void vscode.window.showWarningMessage(
      'Nullscan: the language server is not running. Run "Nullscan: Restart Server" to try again.',
    );
    return undefined;
  }

  private async onConfigurationChanged(
    event: vscode.ConfigurationChangeEvent,
  ): Promise<void> {
    if (!event.affectsConfiguration("nullscan")) {
      return;
    }

    const settings = readSettings();
    this.statusBar.setVisible(settings.showStatusBar);
    if (this.client !== undefined) {
      try {
        await this.client.sendNotification("workspace/didChangeConfiguration", {
          settings: { nullscan: settings },
        });
        await this.refresh();
      } catch (error) {
        this.reportError("could not apply the new settings", error);
      }
    }

    if (event.affectsConfiguration("nullscan.path")) {
      const action = await vscode.window.showInformationMessage(
        "Nullscan: the server path changed. Restart the language server to use it.",
        RESTART_SERVER,
      );
      if (action === RESTART_SERVER) {
        await vscode.commands.executeCommand("nullscan.restartServer");
      }
    }
  }

  private reportError(what: string, error: unknown): void {
    const message = messageOf(error);
    this.output.appendLine(`Nullscan: ${what}: ${message}`);
    void vscode.window.showErrorMessage(`Nullscan: ${what}.`, SHOW_OUTPUT).then((action) => {
      if (action === SHOW_OUTPUT) {
        this.output.show();
      }
    });
  }
}

function runVersion(binaryPath: string): Promise<string> {
  return new Promise((resolve, reject) => {
    execFile(
      binaryPath,
      ["--version"],
      { timeout: VERSION_TIMEOUT_MS, windowsHide: true },
      (error, stdout) => {
        if (error !== null) {
          reject(error);
          return;
        }
        resolve(stdout);
      },
    );
  });
}

async function openNullscanSettings(section: string): Promise<void> {
  await vscode.commands.executeCommand("workbench.action.openSettings", section);
}

async function openDocumentation(url: string): Promise<void> {
  await vscode.env.openExternal(vscode.Uri.parse(url));
}

function firstWorkspaceFolder(): string | undefined {
  return vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
}

async function pickWorkspaceFolder(): Promise<vscode.WorkspaceFolder | undefined> {
  const folders = vscode.workspace.workspaceFolders ?? [];
  if (folders.length === 0) {
    return undefined;
  }
  if (folders.length === 1) {
    return folders[0];
  }
  const picked = await vscode.window.showQuickPick(
    folders.map((folder) => ({
      label: folder.name,
      description: folder.uri.fsPath,
      folder,
    })),
    { placeHolder: "Select the workspace folder for nullscan.toml" },
  );
  return picked?.folder;
}

async function fileExists(uri: vscode.Uri): Promise<boolean> {
  try {
    await vscode.workspace.fs.stat(uri);
    return true;
  } catch {
    return false;
  }
}

function sourceLabel(source: BinarySource): string {
  switch (source) {
    case "setting":
      return "nullscan.path";
    case "bundled":
      return "bundled binary";
    case "download":
      return "downloaded cache";
    case "path":
      return "PATH";
  }
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function firstLine(message: string): string {
  return message.split("\n")[0] ?? message;
}

function isAbortError(error: unknown): boolean {
  if (!(error instanceof Error)) {
    return false;
  }
  return error.name === "AbortError" || /\bcancell?ed\b/i.test(error.message);
}

function formatBytes(bytes: number): string {
  const megabytes = bytes / (1024 * 1024);
  if (megabytes >= 1) {
    return `${megabytes.toFixed(1)} MB`;
  }
  return `${Math.max(1, Math.round(bytes / 1024))} KB`;
}
