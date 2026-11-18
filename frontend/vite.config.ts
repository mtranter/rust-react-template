import { createHmac } from "node:crypto";
import path from "node:path";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

/**
 * The dev sign-in: no identity provider runs in dev, so the proxy signs every `/api` request as
 * the reference modules' seeded Alice (crates/reference/users/api/src/seed.rs) with the `user` role, using the
 * backend's dev-only HS256 secret (`APP_JWT_SECRET`). Only `vite` serves this; production builds
 * carry no token.
 */
function devToken(): string {
  const secret = process.env.APP_JWT_SECRET ?? "dev-only-insecure-secret-change-me";
  const now = Math.floor(Date.now() / 1000);
  const encode = (part: object) => Buffer.from(JSON.stringify(part)).toString("base64url");
  const unsigned = `${encode({ alg: "HS256", typ: "JWT" })}.${encode({
    sub: "00000001-0001-0000-0000-000000000001",
    roles: ["user"],
    iat: now,
    exp: now + 12 * 60 * 60,
  })}`;
  return `${unsigned}.${createHmac("sha256", secret).update(unsigned).digest("base64url")}`;
}

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  server: {
    port: 5173,
    proxy: {
      "/api": {
        target: "http://localhost:8080",
        configure: (proxy) => {
          proxy.on("proxyReq", (request) => {
            if (!request.getHeader("authorization")) {
              request.setHeader("authorization", `Bearer ${devToken()}`);
            }
          });
        },
      },
    },
  },
  test: {
    globals: true,
    environment: "jsdom",
    setupFiles: ["./vitest.setup.ts"],
    // Tests through the router type through whole forms; on a loaded machine one can take past 5s.
    testTimeout: 20_000,
  },
});
