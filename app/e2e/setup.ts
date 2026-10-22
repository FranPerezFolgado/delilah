// Playwright global setup: an isolated daemon (e2e-env) plus Vite serving the UI of this checkout.
import { spawn, type ChildProcess } from "node:child_process";
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { createServer } from "node:net";
import path from "node:path";
import { createInterface } from "node:readline";

const APP = path.resolve(import.meta.dirname, "..");
export const ENV_FILE = path.join(APP, "test-results", "e2e-env.json");
const E2E_ENV = path.resolve(APP, "..", "target", "debug", "e2e-env");

export interface E2eEnv {
  api: string;
  root: string;
  home: string;
  log: string;
  fake_dir: string;
  gh_dir: string;
  jira: string | null;
  fixtures: { repo: string; parent: string };
  ui: string;
}

function freePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const srv = createServer();
    srv.once("error", reject);
    srv.listen(0, "127.0.0.1", () => {
      const { port } = srv.address() as { port: number };
      srv.close(() => resolve(port));
    });
  });
}

function exited(child: ChildProcess): Promise<number | null> {
  return new Promise((resolve) => {
    if (child.exitCode !== null) return resolve(child.exitCode);
    child.once("exit", (code) => resolve(code));
  });
}

async function waitFor(url: string, child: ChildProcess, what: string): Promise<void> {
  const deadline = Date.now() + 60_000;
  while (Date.now() < deadline) {
    if (child.exitCode !== null) throw new Error(`${what} exited early (${child.exitCode})`);
    try {
      if ((await fetch(url)).ok) return;
    } catch {
      // not up yet
    }
    await new Promise((r) => setTimeout(r, 200));
  }
  throw new Error(`${what} did not answer on ${url} within 60 s`);
}

export default async function globalSetup(): Promise<() => Promise<void>> {
  if (!existsSync(E2E_ENV)) {
    throw new Error(
      `${E2E_ENV} is missing: run cargo build -p delilah-daemon -p delilah-e2e -p delilah-cli`,
    );
  }
  const port = await freePort();
  const ui = `http://localhost:${port}`;

  const envProc = spawn(E2E_ENV, ["start", "--jira", "--ui-origin", ui], {
    stdio: ["pipe", "pipe", "inherit"],
  });
  let vite: ChildProcess | undefined;
  const teardown = async () => {
    if (vite && vite.exitCode === null) vite.kill("SIGTERM");
    envProc.stdin?.end();
    const code = await exited(envProc);
    if (code === 3) throw new Error("e2e-env: the tripwire fired (something touched the real Delilah)");
  };

  try {
    const line = await new Promise<string>((resolve, reject) => {
      const rl = createInterface({ input: envProc.stdout! });
      rl.once("line", resolve);
      envProc.once("exit", (c) => reject(new Error(`e2e-env exited (${c}) before printing its JSON line`)));
    });
    const env: E2eEnv = { ...JSON.parse(line), ui };
    // Keep draining stdout so e2e-env never blocks on a full pipe.
    envProc.stdout!.resume();

    // Not npx: the vite binary itself, so the handle we kill is the server.
    vite = spawn(
      path.join(APP, "node_modules", ".bin", "vite"),
      ["--port", String(port), "--strictPort", "--host", "localhost"],
      { cwd: APP, env: { ...process.env, VITE_DELILAH_URL: env.api }, stdio: "ignore" },
    );
    await Promise.all([waitFor(`${env.api}/api/v1/projects`, envProc, "daemon"), waitFor(ui, vite, "vite")]);

    mkdirSync(path.dirname(ENV_FILE), { recursive: true });
    writeFileSync(ENV_FILE, JSON.stringify(env, null, 2));
  } catch (e) {
    await teardown().catch(() => {});
    throw e;
  }
  return teardown;
}
