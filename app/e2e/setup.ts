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

// Ports of the installed app and of the previews: never taken, even when free.
function reserved(port: number): boolean {
  return port === 1420 || port === 7420 || (port >= 1501 && port <= 1509) || (port >= 7501 && port <= 7509);
}

async function freePort(): Promise<number> {
  for (;;) {
    const port = await new Promise<number>((resolve, reject) => {
      const srv = createServer();
      srv.once("error", reject);
      srv.listen(0, "127.0.0.1", () => {
        const { port } = srv.address() as { port: number };
        srv.close(() => resolve(port));
      });
    });
    if (!reserved(port)) return port;
  }
}

/** SIGTERM, then SIGKILL if it has not exited after `ms`. */
async function stop(child: ChildProcess, ms: number): Promise<void> {
  if (child.exitCode !== null || child.signalCode !== null) return;
  const done = new Promise<void>((resolve) => child.once("exit", () => resolve()));
  child.kill("SIGTERM");
  const timer = setTimeout(() => child.kill("SIGKILL"), ms);
  await done;
  clearTimeout(timer);
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

function viteEnv(api: string): NodeJS.ProcessEnv {
  const keep = ["PATH", "HOME", "TMPDIR", "NODE_ENV"];
  const e: NodeJS.ProcessEnv = { VITE_DELILAH_URL: api };
  for (const k of keep) if (process.env[k] !== undefined) e[k] = process.env[k];
  return e;
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
  // Last resort if the runner dies: do not leave our vite behind.
  const killVite = () => {
    if (vite && vite.exitCode === null) vite.kill("SIGKILL");
  };
  process.once("exit", killVite);
  process.once("SIGINT", () => {
    killVite();
    process.exit(130);
  });
  const teardown = async () => {
    if (vite) await stop(vite, 5000);
    envProc.stdin?.end();
    const killer = setTimeout(() => envProc.kill("SIGKILL"), 15_000);
    const code = await exited(envProc);
    clearTimeout(killer);
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
      { cwd: APP, env: viteEnv(env.api), stdio: "ignore" },
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
