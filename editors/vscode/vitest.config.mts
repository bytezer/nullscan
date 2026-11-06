import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["test/**/*.test.ts"],
    // The integration suite needs the VS Code extension host and runs via `vscode-test`.
    exclude: ["test/integration/**", "**/node_modules/**"],
  },
});
