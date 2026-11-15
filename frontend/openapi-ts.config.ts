import { defineConfig } from "@hey-api/openapi-ts";

export default defineConfig({
  input: "./openapi.json",
  output: "./src/api",
  plugins: [
    {
      name: "@hey-api/client-fetch",
      runtimeConfigPath: "./src/hey-api.ts",
    },
    // Names derive from method and path, not the spec's operation ids, which
    // name MCP tools: the two stay independent.
    { name: "@hey-api/sdk", operationId: false },
    "@tanstack/react-query",
  ],
});
