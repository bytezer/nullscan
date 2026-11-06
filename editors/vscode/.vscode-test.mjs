import { defineConfig } from "@vscode/test-cli";
import { buildSync } from "esbuild";

// The integration suite is TypeScript; the extension host only loads CommonJS, so bundle
// it next to the compiled extension and point the runner at the output.
buildSync({
  entryPoints: ["test/integration/extension.test.ts"],
  outfile: "out/test/integration/extension.test.js",
  bundle: true,
  platform: "node",
  format: "cjs",
  target: "node20",
  external: ["vscode"],
  sourcemap: "inline",
  logLevel: "warning",
});

export default defineConfig({
  files: "out/test/integration/**/*.test.js",
  workspaceFolder: "test/fixtures/workspace",
  launchArgs: ["--disable-gpu", "--no-sandbox", "--disable-extensions"],
  mocha: {
    ui: "tdd",
    timeout: 60_000,
  },
});
