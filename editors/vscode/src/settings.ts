import * as vscode from "vscode";

/** The `nullscan.*` settings the client and the server care about. */
export interface NullscanSettings {
  path: string;
  configPath: string;
  analyzeOnOpen: boolean;
  analyzeOnSave: boolean;
  analyzeOnType: boolean;
  includeDuplicates: boolean;
  maxDiagnostics: number;
  showStatusBar: boolean;
}

const SECTION = "nullscan";

/** Reads the `nullscan` section; defaults match the contributions in `package.json`. */
export function readSettings(): NullscanSettings {
  const config = vscode.workspace.getConfiguration(SECTION);
  return {
    path: config.get<string>("path", ""),
    configPath: config.get<string>("configPath", ""),
    analyzeOnOpen: config.get<boolean>("analyzeOnOpen", true),
    analyzeOnSave: config.get<boolean>("analyzeOnSave", true),
    analyzeOnType: config.get<boolean>("analyzeOnType", false),
    includeDuplicates: config.get<boolean>("includeDuplicates", true),
    maxDiagnostics: config.get<number>("maxDiagnostics", 5000),
    showStatusBar: config.get<boolean>("showStatusBar", true),
  };
}
