import * as assert from "node:assert/strict";
import { resolve } from "node:path";

import * as vscode from "vscode";

const EXTENSION_ID = "bytezer.nullscan";
const DIAGNOSTIC_TIMEOUT_MS = 10_000;
const POLL_INTERVAL_MS = 100;

/**
 * End-to-end test of the extension in a real VS Code instance.
 *
 * The runner sets `NULLSCAN_LSP_PATH` (see the command in the README) so the client starts
 * the local build instead of downloading a release.
 */
suite("Nullscan extension", () => {
  const configuration = vscode.workspace.getConfiguration("nullscan");

  suiteSetup(async () => {
    const lspPath = process.env["NULLSCAN_LSP_PATH"];
    if (lspPath === undefined || lspPath.length === 0) {
      throw new Error("NULLSCAN_LSP_PATH must point at a built nullscan-lsp binary.");
    }
    await configuration.update(
      "path",
      resolve(lspPath),
      vscode.ConfigurationTarget.Workspace,
    );

    const extension = vscode.extensions.getExtension(EXTENSION_ID);
    assert.ok(extension !== undefined, `${EXTENSION_ID} is not available`);
    await extension.activate();
  });

  suiteTeardown(async () => {
    // Restore the fixture instead of leaving the local binary path in the workspace.
    await configuration.update("path", "", vscode.ConfigurationTarget.Workspace);
  });

  test("publishes no-console diagnostics for an opened TypeScript file", async () => {
    const folder = vscode.workspace.workspaceFolders?.[0];
    assert.ok(folder !== undefined, "the fixture workspace must be open");

    const document = await vscode.workspace.openTextDocument(
      vscode.Uri.joinPath(folder.uri, "src", "app.ts"),
    );
    await vscode.window.showTextDocument(document);

    const diagnostics = await waitForNullscanDiagnostics(document.uri);
    const console = diagnostics.find(
      (diagnostic) => diagnostic.code === "no-console" && diagnostic.source === "nullscan",
    );
    assert.ok(
      console !== undefined,
      `expected a no-console diagnostic, got ${JSON.stringify(diagnostics)}`,
    );

    const extension = vscode.extensions.getExtension(EXTENSION_ID);
    assert.equal(extension?.isActive, true, "the extension must stay active");
  });
});

async function waitForNullscanDiagnostics(uri: vscode.Uri): Promise<vscode.Diagnostic[]> {
  const deadline = Date.now() + DIAGNOSTIC_TIMEOUT_MS;
  while (Date.now() < deadline) {
    const diagnostics = vscode.languages
      .getDiagnostics(uri)
      .filter((diagnostic) => diagnostic.source === "nullscan");
    if (diagnostics.length > 0) {
      return diagnostics;
    }
    await delay(POLL_INTERVAL_MS);
  }
  throw new Error(
    `no nullscan diagnostics for ${uri.toString()} within ${DIAGNOSTIC_TIMEOUT_MS} ms`,
  );
}

function delay(ms: number): Promise<void> {
  return new Promise((done) => setTimeout(done, ms));
}
