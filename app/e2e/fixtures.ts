import { randomUUID } from "node:crypto";
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { test as base, expect } from "@playwright/test";
import { ENV_FILE, type E2eEnv } from "./setup";

const E2E_ENV_BIN = path.resolve(import.meta.dirname, "..", "..", "target", "debug", "e2e-env");

export interface Api {
  get<T = unknown>(p: string): Promise<T>;
  post<T = unknown>(p: string, body?: unknown): Promise<T>;
  del(p: string): Promise<void>;
}
export interface Fixture {
  repo: string;
  parent: string;
}
export interface Project {
  id: string;
  name: string;
  path: string;
  [k: string]: unknown;
}

export const test = base.extend<{
  env: E2eEnv;
  api: Api;
  fixture: () => Fixture;
  scenario: (name: string, steps: unknown) => void;
  project: (kind?: "repo" | "parent") => Promise<Project>;
  daemonLog: void;
}>({
  env: async ({}, use) => {
    await use(JSON.parse(readFileSync(ENV_FILE, "utf8")) as E2eEnv);
  },
  baseURL: async ({ env }, use) => {
    await use(env.ui);
  },
  api: async ({ env }, use) => {
    const call = async (method: string, p: string, body?: unknown) => {
      const res = await fetch(`${env.api}${p}`, {
        method,
        headers: body === undefined ? {} : { "content-type": "application/json" },
        body: body === undefined ? undefined : JSON.stringify(body),
      });
      if (!res.ok) throw new Error(`${method} ${p}: ${res.status} ${await res.text()}`);
      const text = await res.text();
      return text ? JSON.parse(text) : undefined;
    };
    await use({ get: (p) => call("GET", p), post: (p, b) => call("POST", p, b), del: (p) => call("DELETE", p) as Promise<void> } as Api);
  },
  fixture: async ({}, use) => {
    await use(() => {
      const dir = mkdtempSync(path.join(tmpdir(), "delilah-e2e-fixture-"));
      return JSON.parse(execFileSync(E2E_ENV_BIN, ["fixture", "--into", dir], { encoding: "utf8" })) as Fixture;
    });
  },
  scenario: async ({ env }, use) => {
    await use((name, steps) => {
      if (/[/\\]|\.\./.test(name.replace(/^@/, ""))) throw new Error(`bad scenario name: ${name}`);
      const dir = path.join(env.fake_dir, "scenarios");
      mkdirSync(dir, { recursive: true });
      writeFileSync(path.join(dir, `${name}.json`), JSON.stringify(steps));
    });
  },
  project: async ({ api, fixture }, use) => {
    const made: Project[] = [];
    await use(async (kind = "repo") => {
      const f = fixture();
      const p = await api.post<Project>("/api/v1/projects", {
        path: kind === "repo" ? f.repo : f.parent,
        kind,
        name: `e2e-${kind}-${randomUUID().slice(0, 8)}`,
      });
      made.push(p);
      return p;
    });
    // The daemon is shared by every test: leave no project behind.
    for (const p of made) {
      await api.del(`/api/v1/projects/${encodeURIComponent(p.name)}`).catch(() => {});
    }
  },
  // Auto fixture: on failure attach the tail of the daemon log (FR-015).
  daemonLog: [
    async ({ env }, use, testInfo) => {
      await use();
      if (testInfo.status !== testInfo.expectedStatus) {
        let tail = "";
        try {
          tail = readFileSync(env.log, "utf8").split("\n").slice(-300).join("\n");
        } catch (e) {
          tail = `cannot read ${env.log}: ${e}`;
        }
        await testInfo.attach("daemon-log", { body: tail, contentType: "text/plain" });
      }
    },
    { auto: true },
  ],
});

export { expect };
