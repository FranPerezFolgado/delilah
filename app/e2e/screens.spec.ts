// Screenshots of every screen of the web UI, for comparing it before and after a redesign.
// Not part of the e2e suite: skipped unless DELILAH_SCREENS is `antes` or `despues`, and
// meant to run alone, against the fresh daemon of e2e/setup.ts:
//
//   cargo build -p delilah-daemon -p delilah-cli -p delilah-e2e
//   cd app && DELILAH_SCREENS=antes npx playwright test screens
//
// Images go to docs/redesign/<DELILAH_SCREENS>/<Screen>-oscuro.png / -claro.png (see
// docs/redesign/README.md). Selectors are roles and visible (Spanish) texts, so they
// survive a redesign that keeps the texts; the few that are not are marked `CLASS:`.
import { mkdirSync, renameSync, writeFileSync } from "node:fs";
import path from "node:path";
import type { Browser, BrowserContext, Page } from "@playwright/test";
import { expect, test } from "./fixtures";
import type { E2eEnv } from "./setup";

const MODE = process.env.DELILAH_SCREENS ?? "";
// DELILAH_SCREENS_OUT overrides the output dir (e.g. a temp dir); DELILAH_SCREENS_LANG sets the UI language
// (default "es"); DELILAH_SCREENS_ONLY is a comma-separated list of screen names to capture (default: all).
const OUT_DIR = process.env.DELILAH_SCREENS_OUT ?? "";
const OUT = OUT_DIR ? path.resolve(OUT_DIR) : path.resolve(import.meta.dirname, "..", "..", "docs", "redesign", MODE);
const LANG = process.env.DELILAH_SCREENS_LANG || "es";
const ONLY = (process.env.DELILAH_SCREENS_ONLY ?? "").split(",").map((n) => n.trim()).filter(Boolean);

const THEMES = [
  { scheme: "dark", suffix: "oscuro" },
  { scheme: "light", suffix: "claro" },
] as const;
type Theme = (typeof THEMES)[number];

// Keys are LIBRE-<n> in creation order on a fresh daemon: the seed creates them in this order.
const K = {
  backlogChat: "LIBRE-1",
  backlogPty: "LIBRE-2",
  backlogParent: "LIBRE-3",
  done: "LIBRE-4",
  live: "LIBRE-5",
  running: "LIBRE-6",
  paused: "LIBRE-7",
  setup: "LIBRE-8",
  jira: "PROJ-1246",
} as const;

/** Seed text: Spanish by default, English when DELILAH_SCREENS_LANG=en. */
const tr = (es: string, en: string) => (LANG === "en" ? en : es);

const LIVE_TITLE = tr("Chat: tarjetas de permiso en vivo", "Chat: live permission cards");
const BRANCH = {
  done: "feature/libre-4-reintentos-webhook",
  live: "feature/libre-5-tarjetas-permiso",
  running: "feature/libre-6-paginacion-cursor",
  paused: "feature/libre-7-metrica-latencia",
  setup: "feature/libre-8-common-lib-java-21",
};

test.describe.configure({ mode: "serial" });
test.skip(
  MODE !== "antes" && MODE !== "despues" && !OUT_DIR,
  "set DELILAH_SCREENS=antes|despues (or DELILAH_SCREENS_OUT) to take the screenshots",
);
test.use({ locale: "es-ES", timezoneId: "Europe/Madrid" });

// ---------- daemon helpers (the fixtures' Api has no PUT) ----------

async function call<T = unknown>(env: E2eEnv, method: string, p: string, body?: unknown): Promise<T> {
  const res = await fetch(`${env.api}${p}`, {
    method,
    headers: body === undefined ? {} : { "content-type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  if (!res.ok) throw new Error(`${method} ${p}: ${res.status} ${await res.text()}`);
  const text = await res.text();
  return (text ? JSON.parse(text) : undefined) as T;
}

interface TaskJson {
  key: string;
  branch?: string | null;
  status: { type: string };
}

async function statusOf(env: E2eEnv, key: string): Promise<string> {
  return (await call<TaskJson>(env, "GET", `/api/v1/tasks/${key}`)).status.type;
}

async function waitStatus(env: E2eEnv, key: string, want: string) {
  await expect.poll(() => statusOf(env, key), { message: `${key} becomes ${want}`, timeout: 30_000 }).toBe(want);
}

// ---------- fake gh (same shapes as crates/delilah-e2e/src/fixtures.rs FakeGh) ----------

const REPO_URL = "https://github.com/o/r";

function put(dir: string, name: string, value: unknown) {
  const tmp = path.join(dir, `.${name}.tmp`);
  writeFileSync(tmp, JSON.stringify(value, null, 2));
  renameSync(tmp, path.join(dir, name));
}

interface FakePr {
  n: number;
  branch: string;
  title: string;
  state: "OPEN" | "MERGED";
  ci: "SUCCESS" | "FAILURE";
  review?: string;
  comment?: string;
}

function writePrs(dir: string, prs: FakePr[]) {
  mkdirSync(dir, { recursive: true });
  put(dir, "repo.json", { nameWithOwner: "o/r", url: REPO_URL, defaultBranchRef: { name: "main" } });
  put(dir, "user.json", { login: "e2e-user" });
  put(
    dir,
    "pr-list.json",
    prs.map((p) => ({
      number: p.n, url: `${REPO_URL}/pull/${p.n}`, state: p.state, updatedAt: "2026-10-20T10:00:00Z",
      headRefName: p.branch, baseRefName: "main", title: p.title, isDraft: false,
    })),
  );
  for (const p of prs) {
    put(dir, `pr-${p.n}.json`, {
      data: {
        repository: {
          pullRequest: {
            state: p.state, isDraft: false, merged: p.state === "MERGED", headRefOid: `sha-${p.n}-1`,
            baseRefName: "main", title: p.title, url: `${REPO_URL}/pull/${p.n}`,
            reviewDecision: p.review ? "CHANGES_REQUESTED" : p.state === "MERGED" ? "APPROVED" : null,
            commits: {
              nodes: [
                {
                  commit: {
                    statusCheckRollup: {
                      contexts: {
                        nodes: [
                          { __typename: "CheckRun", name: "build", status: "COMPLETED", conclusion: p.ci, detailsUrl: "https://ci.invalid/1" },
                          { __typename: "CheckRun", name: "lint", status: "COMPLETED", conclusion: "SUCCESS", detailsUrl: "https://ci.invalid/2" },
                        ],
                      },
                    },
                  },
                },
              ],
            },
            reviews: {
              nodes: p.review
                ? [{ databaseId: 100 + p.n, state: "CHANGES_REQUESTED", author: { login: "reviewer" }, body: p.review, submittedAt: "2026-10-20T10:00:00Z" }]
                : [],
            },
            comments: {
              nodes: p.comment
                ? [{ databaseId: 200 + p.n, author: { login: "reviewer" }, body: p.comment, createdAt: "2026-10-20T10:30:00Z" }]
                : [],
            },
            reviewThreads: { nodes: [] },
          },
        },
      },
    });
  }
}

// ---------- page helpers ----------

/** Shared between the serial tests of this file (one worker). */
const seeded = { shellLabel: "" };

async function open(browser: Browser, env: E2eEnv, theme: Theme, width = 1440, height = 900): Promise<{ ctx: BrowserContext; page: Page }> {
  const ctx = await browser.newContext({
    baseURL: env.ui,
    viewport: { width, height },
    colorScheme: theme.scheme,
    locale: "es-ES",
    timezoneId: "Europe/Madrid",
    reducedMotion: "reduce",
  });
  const page = await ctx.newPage();
  await page.goto("/");
  await expect(nav(page).getByRole("button", { name: /^(Bandeja|Inbox)/ })).toBeVisible();
  // The language preference is read after the first render.
  await expect(nav(page).getByRole("button", { name: /^(Ajustes|Settings)/ })).toBeVisible();
  return { ctx, page };
}

const nav = (page: Page) => page.getByRole("navigation", { name: /^(Navegación|Navigation)$/ });

/** Waits for the screen to have nothing loading, fonts in and a frame painted. */
async function settle(page: Page) {
  await expect(page.getByText("Cargando…")).toHaveCount(0);
  await page.evaluate(() => document.fonts.ready.then(() => undefined));
  await page.mouse.move(0, 0);
  await page.waitForTimeout(500);
}

async function shot(page: Page, name: string, theme: Theme) {
  if (ONLY.length && !ONLY.includes(name)) return;
  await settle(page);
  mkdirSync(OUT, { recursive: true });
  await page.screenshot({ path: path.join(OUT, `${name}-${theme.suffix}.png`), animations: "disabled", caret: "hide" });
}

/** A task's row in the sidebar. */
const row = (page: Page, key: string) => nav(page).getByRole("button", { name: new RegExp(`^${key}\\b`) });

/** The board, by its shortcut (⌘1): the sidebar's «Tareas» shows the selected task's screen. */
async function board(page: Page) {
  await page.keyboard.press("ControlOrMeta+1");
  await expect(page.getByRole("heading", { name: /^(Tareas|Tasks)$/ })).toBeVisible();
}

async function openTask(page: Page, key: string) {
  await row(page, key).click();
  await expect(page.getByRole("tab", { name: "Tarea", exact: true })).toBeVisible();
}

async function chatTab(page: Page) {
  await page.getByRole("tab", { name: /^(Claude principal|Principal|Main)$/ }).click();
  await expect(page.getByRole("textbox", { name: "Mensaje" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Permitir una vez" }).first()).toBeVisible();
}

async function jiraWithTicket(page: Page) {
  await nav(page).getByRole("button", { name: /^Jira/ }).click();
  await page.getByText("PROJ-1242", { exact: true }).first().click();
  await expect(page.getByRole("button", { name: "Analizar con Claude" })).toBeVisible();
  await expect(page.getByText("Descripción", { exact: true })).toBeVisible();
}

// ---------- the seed ----------

test("seed: projects, tasks in every state, PRs, a terminal", async ({ env, scenario }) => {
  test.setTimeout(180_000);
  const tasks = await call<unknown[]>(env, "GET", "/api/v1/tasks");
  const projects = await call<unknown[]>(env, "GET", "/api/v1/projects");
  if (tasks.length || projects.length) {
    throw new Error("the daemon is not empty: run this spec alone (npx playwright test screens)");
  }
  await call(env, "PUT", "/api/v1/preferences/language", LANG);
  await call(env, "POST", "/api/v1/projects", { path: env.fixtures.repo, kind: "repo", name: "signal" });
  await call(env, "POST", "/api/v1/projects", { path: env.fixtures.parent, kind: "parent", name: "delilah" });
  // The «antes» capture ran when a project defaulted to isolated worktrees; the default is the checkout now.
  for (const p of await call<{ name: string; settings: Record<string, unknown> }[]>(env, "GET", "/api/v1/projects")) {
    await call(env, "PUT", `/api/v1/projects/${p.name}/settings`, { ...p.settings, workspace: "worktree" });
  }

  // PRs first: the daemon links them by branch as soon as the tasks exist.
  writePrs(env.gh_dir, [
    { n: 12, branch: BRANCH.done, title: tr("Reintentos con backoff en el webhook de Stripe", "Retries with backoff in the Stripe webhook"), state: "MERGED", ci: "SUCCESS" },
    {
      n: 14, branch: BRANCH.live, title: LIVE_TITLE, state: "OPEN", ci: "FAILURE",
      review: tr("El test de la tarjeta de permiso falla en CI, ¿lo miras?", "The permission card test fails in CI, can you take a look?"), comment: tr("¿Y si el permiso caduca mientras espera?", "What if the permission expires while it waits?"),
    },
  ]);

  // A terminal agent working for minutes (sleep_ms is capped at 5 s per step).
  scenario("screens-running", {
    steps: [
      { print: tr("Leyendo el plan de la spec…\r\nImplementando la paginación por cursor en GET /invoices\r\n", "Reading the spec plan…\r\nImplementing cursor pagination in GET /invoices\r\n") },
      ...Array.from({ length: 180 }, () => ({ sleep_ms: 5000 })),
    ],
  });
  // A turn that fails on the usage limit: paused until the reset (now + 30 min, unknown reset).
  scenario("screens-paused", {
    steps: [{ print: tr("Midiendo la latencia del gateway…\r\n", "Measuring the gateway latency…\r\n") }, { fail: "API Error: 429 rate_limit_error: usage limit reached" }],
  });
  // Chat mode: one tool already allowed, then a permission card left waiting.
  scenario("screens-live", {
    turns: [
      [
        { say: { text: tr("Vale. Voy a ejecutar los comandos uno a uno y esperar tu respuesta a cada permiso.", "Okay. I will run the commands one by one and wait for your answer to each permission.") } },
        { tool: { name: "Bash", input: { command: "git status --short" }, output: " M README.md", ask: false } },
        { say: { text: tr("Hay un cambio en README.md. Ahora listo el directorio.", "There is a change in README.md. Now I list the directory.") } },
        { tool: { name: "Bash", input: { command: "ls -la" }, output: "total 8\nREADME.md\nsrc" } },
        { say: { text: tr("Listado hecho.", "Listing done.") } },
      ],
    ],
  });

  const backlog = [
    { title: tr("Chat: probar «Permitir siempre» con prefijos", "Chat: try \"Always allow\" with prefixes"), project: "signal", chat_mode: true,
      notes: tr("Pide ejecutar ls -la, cat README.md y luego cat ~/.ssh/config: el último debe volver a preguntar.", "Ask to run ls -la, cat README.md and then cat ~/.ssh/config: the last one must ask again.") },
    { title: tr("Terminal: tarea clásica en PTY", "Terminal: classic task in a PTY"), project: "signal", notes: tr("Sin modo chat, para comparar la vista.", "No chat mode, to compare the view.") },
    { title: tr("Parent: tocar svc-orders sin rama", "Parent: touch svc-orders without a branch"), project: "delilah", chat_mode: true,
      notes: tr("Tarea libre sin rama: un git -C svc-orders commit debe crear la rama antes.", "Free task without a branch: a git -C svc-orders commit must create the branch first.") },
  ];
  for (const [i, b] of backlog.entries()) {
    const t = await call<TaskJson>(env, "POST", "/api/v1/tasks", { key: "", free: true, ...b });
    expect(t.key).toBe(`LIBRE-${i + 1}`);
  }

  const run = async (want: string, body: Record<string, unknown>) => {
    const t = await call<TaskJson>(env, "POST", "/api/v1/runs", { key: "", free: true, submodules: [], project: "signal", ...body });
    expect(t.key).toBe(want);
    return t;
  };
  // Done, with a merged PR: the default scenario stops and waits; then marked done.
  const done = await run(K.done, { title: tr("Reintentos con backoff en el webhook de Stripe", "Retries with backoff in the Stripe webhook"), branch: BRANCH.done });
  expect(done.branch).toBe(BRANCH.done);
  const live = await run(K.live, { title: LIVE_TITLE, branch: BRANCH.live, chat_mode: true, prompt: "Hola. Para probar las tarjetas de permiso, ejecuta uno a uno y espera mi respuesta a cada permiso: git status, ls -la, cat README.md. [fake:screens-live]" });
  expect(live.branch).toBe(BRANCH.live);
  await run(K.running, { title: tr("Paginación por cursor en GET /invoices", "Cursor pagination in GET /invoices"), branch: BRANCH.running, prompt: "Implementa [fake:screens-running]" });
  await run(K.paused, { title: tr("Métrica de latencia del gateway de pagos", "Payments gateway latency metric"), branch: BRANCH.paused, prompt: "Mide [fake:screens-paused]" });
  // A Jira task in the parent project, waiting for the user (default scenario).
  await call(env, "POST", "/api/v1/runs", {
    key: K.jira, title: tr("Idempotency-Key en POST /payments", "Idempotency-Key on POST /payments"), project: "delilah", submodules: ["svc-payments"],
  });

  // Setup failed: a failing setup command only while this task starts.
  type Proj = { name: string; settings: Record<string, unknown> };
  const signal = (await call<Proj[]>(env, "GET", "/api/v1/projects")).find((p) => p.name === "signal")!;
  await call(env, "PUT", "/api/v1/projects/signal/settings", {
    ...signal.settings,
    setup: "echo '> npm ci'; echo 'npm ERR! code ENOENT: no se encuentra package-lock.json' >&2; exit 1",
  });
  await run(K.setup, { title: tr("Actualizar common-lib a Java 21", "Upgrade common-lib to Java 21"), branch: BRANCH.setup });
  await waitStatus(env, K.setup, "setup-failed");
  await call(env, "PUT", "/api/v1/projects/signal/settings", signal.settings);

  await waitStatus(env, K.running, "running");
  await waitStatus(env, K.paused, "paused-by-limit");
  await waitStatus(env, K.jira, "waiting-for-user");
  await expect
    .poll(async () => JSON.stringify(await call(env, "GET", "/api/v1/inbox")), { message: "the permission card in the inbox", timeout: 30_000 })
    .toContain("ls -la");

  // Phases, so the board has something in each column.
  for (const [key, phase] of [[K.live, "implement"], [K.running, "plan"], [K.paused, "implement"], [K.jira, "review"], [K.done, "review"]] as const) {
    await call(env, "POST", `/api/v1/tasks/${key}/phase`, { phase });
  }

  // Changes in LIBRE-5's worktree (the file API only saves existing files).
  const readme = await call<{ text: string; version: string }>(env, "GET", `/api/v1/tasks/${K.live}/file?path=README.md`);
  await call(env, "PUT", `/api/v1/tasks/${K.live}/file?path=README.md`, {
    text: `${readme.text}\n## Permisos en el chat\n\nCada herramienta pide permiso con una tarjeta: permitir una vez, siempre o denegar.\n`,
    version: readme.version,
  });

  // A terminal next to the agent.
  const shell = await call<{ label: string }>(env, "POST", `/api/v1/tasks/${K.live}/sessions`, { kind: "shell" });
  seeded.shellLabel = shell.label;

  // LIBRE-5's PR is linked, then LIBRE-4's: its merged PR marks it done by itself. The fake gh
  // answers `--head <branch>` with that branch's PR only, and a lookup below review needs
  // `refresh`; linking moves LIBRE-5 to review, as it was when the «antes» was taken.
  await call(env, "POST", `/api/v1/tasks/${K.live}/prs/refresh`, {});
  await expect
    .poll(async () => (await call<{ task: string }[]>(env, "GET", "/api/v1/prs")).some((p) => p.task === K.live), { message: "LIBRE-5's PR linked", timeout: 30_000 })
    .toBe(true);
  await waitStatus(env, K.done, "waiting-for-user");
  await call(env, "POST", `/api/v1/tasks/${K.done}/prs/refresh`, {});
  await waitStatus(env, K.done, "done");
});

// ---------- Jira's connect form, before it is connected ----------

test("Jira-conectar", async ({ browser, env }) => {
  test.skip(ONLY.length > 0 && !ONLY.includes("Jira-conectar"), "not in DELILAH_SCREENS_ONLY");
  for (const theme of THEMES) {
    const { ctx, page } = await open(browser, env, theme);
    await nav(page).getByRole("button", { name: /^Jira/ }).click();
    await expect(page.getByRole("button", { name: /Conectar/ }).first()).toBeVisible();
    await shot(page, "Jira-conectar", theme);
    await ctx.close();
  }
  await call(env, "PUT", "/api/v1/jira", { mode: "sample" });
});

// ---------- every screen, per theme ----------

for (const theme of THEMES) {
  test(`pantallas (${theme.suffix})`, async ({ browser, env }) => {
    test.setTimeout(240_000);
    const { ctx, page } = await open(browser, env, theme);

    // Tablero
    await board(page);
    await expect(page.getByText(LIVE_TITLE).last()).toBeVisible();
    await shot(page, "Tablero", theme);
    // The rest of the flow drives Spanish UI text; stop here when only the board was asked for.
    if (ONLY.length && ONLY.every((n) => n === "Tablero")) {
      await ctx.close();
      return;
    }

    // Bandeja
    await nav(page).getByRole("button", { name: /^Bandeja/ }).click();
    await expect(page.getByRole("heading", { name: "Bandeja" })).toBeVisible();
    await expect(page.getByRole("button", { name: "Permitir una vez" }).first()).toBeVisible();
    await shot(page, "Bandeja", theme);

    // Tarea: overview, chat, terminal, Cambios, PR
    await openTask(page, K.live);
    await page.getByRole("tab", { name: "Tarea", exact: true }).click();
    await expect(page.getByRole("button", { name: "Abrir cambios" }).first()).toBeVisible();
    await shot(page, "Tarea-tarea", theme);

    await chatTab(page);
    await shot(page, "Tarea-chat", theme);

    await page.getByRole("tab", { name: seeded.shellLabel || /zsh|Terminal/ }).click();
    // CLASS: xterm's own class for the terminal's input; no role to reach it.
    await page.locator(".xterm-helper-textarea").focus();
    await page.keyboard.type("export PS1='%1~ %# '; clear; git status -sb; ls\n");
    await page.waitForTimeout(1500);
    await shot(page, "Tarea-zsh", theme);

    await page.getByRole("tab", { name: "Tarea", exact: true }).click();
    await page.getByRole("button", { name: "Abrir cambios" }).first().click();
    await expect(page.getByRole("tab", { name: "Cambios" })).toBeVisible();
    await expect(page.getByText("README.md").first()).toBeVisible();
    await shot(page, "Tarea-cambios", theme);

    await page.getByRole("tab", { name: "PR", exact: true }).click();
    await expect(page.getByText("Cambios pedidos").first()).toBeVisible({ timeout: 30_000 });
    await shot(page, "Tarea-pr", theme);

    // Quick Open, over the task screen
    await chatTab(page);
    await page.keyboard.press("ControlOrMeta+p");
    const qo = page.getByRole("dialog", { name: "Abrir archivo" });
    await expect(qo).toBeVisible();
    await qo.getByRole("textbox", { name: "Nombre del archivo" }).fill("READ");
    await expect(qo.getByRole("option").first()).toContainText("README.md");
    await shot(page, "QuickOpen", theme);
    await page.keyboard.press("Escape");
    await expect(qo).toBeHidden();

    // Jira, a ticket selected
    await jiraWithTicket(page);
    await shot(page, "Jira", theme);

    // Proyectos and a project's own screen
    await nav(page).getByRole("button", { name: /^Proyectos/ }).click();
    await expect(page.getByRole("heading", { name: "Proyectos" })).toBeVisible();
    await shot(page, "Proyectos", theme);

    await nav(page).getByRole("button", { name: "signal", exact: true }).click();
    await expect(page.getByRole("tab", { name: "Proyecto", exact: true })).toBeVisible();
    await shot(page, "ProyectoVista", theme);

    // Ajustes
    await nav(page).getByRole("button", { name: /^Ajustes/ }).click();
    await expect(page.getByRole("heading", { name: "Ajustes" })).toBeVisible();
    await shot(page, "Ajustes", theme);

    // Nueva tarea, each origin, over the board
    await board(page);
    await page.getByRole("button", { name: "Nueva tarea", exact: true }).click();
    const dialog = page.getByRole("dialog", { name: "Nueva tarea" });
    await expect(dialog).toBeVisible();
    for (const [mode, label] of [["jira", "Ticket de Jira"], ["libre", "Tarea libre"], ["claude", "Con Claude"]] as const) {
      await dialog.getByRole("radio", { name: label }).click();
      await expect(dialog.getByRole("radio", { name: label })).toHaveAttribute("aria-checked", "true");
      await shot(page, `NuevaTarea-${mode}`, theme);
    }
    await page.keyboard.press("Escape");
    await expect(dialog).toBeHidden();

    // A backlog task
    await nav(page).getByRole("button", { name: /^Backlog · 2/ }).click();
    await row(page, K.backlogChat).click();
    await expect(page.getByRole("button", { name: "Iniciar" })).toBeVisible();
    await shot(page, "BacklogTarea", theme);

    // Delete dialog, from the row's context menu (cancelled)
    await row(page, K.backlogPty).click({ button: "right" });
    await page.getByRole("menuitem", { name: "Eliminar tarea…" }).click();
    const del = page.getByRole("dialog", { name: `¿Eliminar ${K.backlogPty}?` });
    await expect(del).toBeVisible();
    await shot(page, "Eliminar", theme);
    await del.getByRole("button", { name: "Cancelar" }).click();
    await expect(del).toBeHidden();

    // Logs: the statusbar's button
    await page.getByRole("button", { name: "Logs", exact: true }).click();
    const logs = page.getByRole("dialog", { name: "Logs de delilahd" });
    await expect(logs).toBeVisible();
    await expect(logs.getByText("Sin logs todavía.")).toHaveCount(0);
    await shot(page, "Logs", theme);
    await logs.getByRole("button", { name: "Cerrar" }).click();

    // A task whose setup failed
    await openTask(page, K.setup);
    await expect(page.getByRole("button", { name: "Reintentar setup" })).toBeVisible();
    await shot(page, "TareaSetup", theme);
    await ctx.close();

    // Smaller windows
    {
      const { ctx, page } = await open(browser, env, theme, 1280, 800);
      await board(page);
      await expect(page.getByText(LIVE_TITLE).last()).toBeVisible();
      await shot(page, "Tablero1280", theme);
      // At this width the sidebar folds into the rail, which has no task rows: from the board.
      await page.getByRole("link", { name: new RegExp(`^${K.live}\\b`) }).click();
      await expect(page.getByRole("tab", { name: "Tarea", exact: true })).toBeVisible();
      await chatTab(page);
      await shot(page, "Tarea1280", theme);
      await ctx.close();
    }
    {
      const { ctx, page } = await open(browser, env, theme, 1024, 700);
      await jiraWithTicket(page);
      await shot(page, "Jira1024", theme);
      await ctx.close();
    }
  });
}
