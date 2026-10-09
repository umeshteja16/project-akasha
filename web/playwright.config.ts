// End-to-end tests against the real stack: Postgres + `akasha serve` (with the
// built UI embedded, deterministic models, fake LLM). Run with `just e2e`.
//
// Env: DATABASE_URL (defaults to the dev database), AKASHA_BIN (server binary,
// default ../target/debug/akasha built with `--features embed-ui`),
// AKASHA_E2E_PORT (default 8091), E2E_BASE_URL (skip starting a server).

import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { defineConfig, devices } from "@playwright/test";

const port = Number(process.env.AKASHA_E2E_PORT ?? 8091);
const baseURL = process.env.E2E_BASE_URL ?? `http://127.0.0.1:${port}`;
const binary = process.env.AKASHA_BIN ?? "../target/debug/akasha";

export default defineConfig({
  testDir: "./e2e",
  fullyParallel: false,
  workers: 1,
  forbidOnly: Boolean(process.env.CI),
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [["list"], ["html", { open: "never" }]] : "list",
  timeout: 30_000,
  use: {
    baseURL,
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
  },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
  webServer: process.env.E2E_BASE_URL
    ? undefined
    : {
        command: `${binary} serve --with-worker`,
        url: `${baseURL}/readyz`,
        timeout: 60_000,
        reuseExistingServer: false,
        stdout: "pipe",
        stderr: "pipe",
        env: {
          DATABASE_URL:
            process.env.DATABASE_URL ?? "postgres://akasha:akasha@localhost:5432/akasha",
          AKASHA_BIND_ADDR: `127.0.0.1:${port}`,
          AKASHA_LOG_FORMAT: "pretty",
          AKASHA_ALLOW_REGISTRATION: "true",
          AKASHA_COOKIE_SECURE: "false",
          AKASHA_STORAGE_DIR: mkdtempSync(join(tmpdir(), "akasha-e2e-")),
          // Deterministic built-in models: no downloads, no ONNX Runtime.
          AKASHA_EMBED_MODEL: "hash-384",
          AKASHA_RERANK_MODEL: "overlap",
          AKASHA_ML_MODELS_URL: "",
          AKASHA_OCR_ENABLED: "false",
          AKASHA_LLM_PROVIDER: "fake",
        },
      },
});
