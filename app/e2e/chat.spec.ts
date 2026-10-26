import { readFileSync } from "node:fs";
import path from "node:path";
import type { Page } from "@playwright/test";
import { boardCard, expect, test, type Api, type Project } from "./fixtures";
import type { E2eEnv } from "./setup";

// Chat mode: the fake Claude speaks stream-json when run with `--input-format stream-json`
// and plays the scenario's `turns`, one per user message (src/fake/stream.rs).
// The UI follows the browser language and these specs read the Spanish texts.
test.use({ locale: "es-ES", viewport: { width: 1440, height: 900 } });

interface Task {
  key: string;
  title: string;
  chat_mode: boolean;
  allowed_tools?: string[];
}
interface Session {
  id: string;
  task: string;
  kind: string;
  live: boolean;
  closed: boolean;
  runtime?: "pty" | "stream";
}
interface FakeRecord {
  inputs: string[];
  permissions: { tool: string; input: Record<string, unknown>; response: Record<string, unknown> | null }[];
}

const uid = () => `${Date.now()}-${Math.floor(Math.random() * 1e6)}`;

/** Starts a free task in chat mode whose fake plays `turns`. */
async function startChat(
  api: Api,
  scenario: (name: string, steps: unknown) => void,
  p: Project,
  turns: unknown[][],
  label = "Chat",
): Promise<{ key: string; title: string; name: string }> {
  const name = `chat-${uid()}`;
  scenario(name, { turns });
  const title = `${label} ${uid()}`;
  const task = await api.post<Task>("/api/v1/runs", {
    key: "",
    title,
    project: p.name,
    submodules: [],
    free: true,
    chat_mode: true,
    prompt: `Hola [fake:${name}]`,
  });
  return { key: task.key, title, name };
}

async function primarySession(api: Api, key: string): Promise<Session> {
  let found: Session | undefined;
  await expect
    .poll(async () => {
      const all = await api.get<Session[]>("/api/v1/sessions");
      found = all.find((s) => s.task === key && s.kind === "claude" && !s.closed);
      return !!found;
    })
    .toBe(true);
  return found!;
}

/** The fake's record for `session` (what it read and was answered). */
function record(env: E2eEnv, session: string): FakeRecord {
  const file = path.join(env.fake_dir, "records", `${session}.json`);
  try {
    return JSON.parse(readFileSync(file, "utf8")) as FakeRecord;
  } catch {
    return { inputs: [], permissions: [] };
  }
}

/** The runtime of the task's running Claude session: "pty", "stream" or "none". */
async function liveRuntime(api: Api, key: string): Promise<string> {
  const live = (await api.get<Session[]>("/api/v1/sessions")).find((x) => x.task === key && x.kind === "claude" && x.live);
  return live ? (live.runtime ?? "pty") : "none";
}

async function openTask(page: Page, title: string) {
  await page.goto("/");
  await page.getByRole("button", { name: /^Tareas/ }).click();
  await (await boardCard(page, title)).click();
  // The primary Claude session's tab.
  await page.getByRole("tab", { name: "Claude principal", exact: true }).click();
}

test("a chat-mode task shows the chat view, live text grows in a bubble and becomes the final message", async ({
  page,
  project,
  scenario,
  api,
}) => {
  const p = await project("repo");
  const words = "Uno dos tres cuatro cinco seis siete ocho nueve diez once doce";
  const { key, title } = await startChat(api, scenario, p, [
    [{ say: { text: "Primer turno listo." } }],
    [{ say: { text: words, delay_ms: 250 } }],
  ]);
  const s = await primarySession(api, key);
  expect(s.runtime).toBe("stream");

  await openTask(page, title);
  const conv = page.locator(".conv");
  await expect(conv.locator(".conv-claude", { hasText: "Primer turno listo." })).toBeVisible({ timeout: 30_000 });

  await conv.getByRole("textbox", { name: "Mensaje" }).fill("Cuenta hasta doce");
  await conv.getByRole("button", { name: /^Enviar/ }).click();

  // The bubble grows while the deltas come...
  const bubble = conv.getByTestId("writing");
  await expect(bubble).toContainText("Uno dos");
  const early = (await bubble.innerText()).length;
  await expect.poll(async () => (await bubble.innerText().catch(() => "")).length).toBeGreaterThan(early);
  // ...and is replaced by the transcript's message once it is complete.
  await expect(conv.locator(".conv-claude:not(.conv-writing)", { hasText: words })).toBeVisible();
  await expect(bubble).toHaveCount(0);
  await expect(conv.locator(".conv-claude", { hasText: words })).toHaveCount(1);

  // A chat-mode session: the switch shows Chat, and Terminal is the way back to a terminal.
  const sw = page.getByRole("group", { name: "Vista de la sesión" });
  await expect(sw.getByRole("button", { name: /^Chat/ })).toHaveAttribute("aria-pressed", "true");
  await expect(sw.getByRole("button", { name: "Terminal", exact: true })).toBeEnabled();
});

// These tests are about the permission card, so each command is one Claude's auto mode
// escalates (`auto: "ask"`); under the default Auto setting a routine one would just run.
const bash = (command: string, output = "", extra: Record<string, unknown> = {}) => ({
  tool: { name: "Bash", input: { command }, output, auto: "ask", ...extra },
});
const say = (text: string) => ({ say: { text } });

async function send(page: Page, text: string) {
  const conv = page.locator(".conv");
  await conv.getByRole("textbox", { name: "Mensaje" }).fill(text);
  await conv.getByRole("button", { name: /^Enviar/ }).click();
}

test("a permission card shows in the chat and the inbox; allow once runs the tool, deny reaches the agent", async ({
  page,
  project,
  scenario,
  api,
  env,
}) => {
  const p = await project("repo");
  const listing = "total 8\ndrwxr-xr-x  4 me  staff  128 README.md";
  const { key, title } = await startChat(api, scenario, p, [
    [bash("ls -la", listing), say("Listado hecho.")],
    [bash("rm -rf build"), say("Vale, no lo borro.")],
  ]);
  const s = await primarySession(api, key);

  // The ask waits in the inbox ("Bandeja") with its own card.
  await page.goto("/");
  await page.getByRole("button", { name: /^Bandeja/ }).click();
  const row = page.getByRole("option").filter({ hasText: "ls -la" });
  await expect(row).toBeVisible({ timeout: 30_000 });
  await row.click();
  // The detail is the permission card.
  await expect(page.locator(".ib-detail .ac").filter({ hasText: "ls -la" })).toBeVisible();
  await expect(page.locator(".ib-detail").getByRole("button", { name: "Permitir una vez" })).toBeVisible();

  await page.getByRole("button", { name: /^Tareas/ }).click();
  await (await boardCard(page, title)).click();
  await page.getByRole("tab", { name: "Claude principal", exact: true }).click();
  const conv = page.locator(".conv");
  const card = conv.locator(".ac-chat", { hasText: "ls -la" });
  await expect(card).toBeVisible();
  await card.getByRole("button", { name: "Permitir una vez" }).click();

  // The answered card stays in the log as history, without buttons.
  await expect(card).toContainText("Permitida una vez");
  await expect(card.getByRole("button")).toHaveCount(0);
  await expect(conv.locator(".conv-claude", { hasText: "Listado hecho." })).toBeVisible();
  expect(record(env, s.id).permissions[0]?.response).toMatchObject({ behavior: "allow" });
  // The tool card: the command and what it printed.
  const tool = conv.getByRole("group", { name: /ls -la/ });
  await tool.getByRole("button").first().click();
  await expect(tool.locator(".tool-cmd")).toHaveText("ls -la");
  await expect(tool.locator(".tool-out")).toContainText("README.md");

  await send(page, "Borra build");
  const second = conv.locator(".ac-chat", { hasText: "rm -rf build" });
  await expect(second).toBeVisible();
  await second.getByRole("button", { name: "Denegar" }).click();
  await expect(conv.locator(".conv-claude", { hasText: "Vale, no lo borro." })).toBeVisible();
  await expect.poll(() => record(env, s.id).permissions[1]?.response).toMatchObject({
    behavior: "deny",
  });
  // The denied call failed with the reason as its result.
  const denied = conv.getByRole("group", { name: /rm -rf build/ });
  await expect(denied).toHaveClass(/failed/);
});

test("allow always remembers the exact rule and the daemon answers the same request itself; a prefix rule stays bounded", async ({
  page,
  project,
  scenario,
  api,
  env,
}) => {
  const p = await project("repo");
  const prefix = [{ type: "addRules", behavior: "allow", destination: "localSettings", rules: [{ toolName: "Bash", ruleContent: "git diff:*" }] }];
  const { key, title } = await startChat(api, scenario, p, [
    [bash("npm --version", "10.9.9"), say("Versión vista.")],
    [bash("npm --version", "10.9.9"), say("Otra vez sin preguntar.")],
    [bash("git diff HEAD", "", { suggestions: prefix }), say("Diff visto.")],
    [bash("git diff --stat", " 1 file changed", { suggestions: prefix }), say("Stat visto.")],
    [bash("git diff --output=../x", "", { suggestions: prefix }), say("Salida denegada.")],
  ]);
  const s = await primarySession(api, key);
  await openTask(page, title);
  const conv = page.locator(".conv");

  const first = conv.locator(".ac-chat", { hasText: "npm --version" });
  await expect(first.locator(".ac-rule")).toHaveText("Bash(npm --version)", { timeout: 30_000 });
  await first.getByRole("button", { name: "Permitir siempre en esta tarea" }).click();
  await expect(conv.locator(".conv-claude", { hasText: "Versión vista." })).toBeVisible();
  await expect.poll(async () => (await api.get<Task>(`/api/v1/tasks/${key}`)).allowed_tools).toEqual(["Bash(npm --version)"]);

  // The same request again: allowed by the daemon, no card.
  await send(page, "Otra vez");
  await expect(conv.locator(".conv-claude", { hasText: "Otra vez sin preguntar." })).toBeVisible();
  await expect(conv.getByRole("button", { name: "Permitir una vez" })).toHaveCount(0);
  expect(record(env, s.id).permissions[1]?.response).toMatchObject({ behavior: "allow" });

  // Claude suggests a prefix rule on the safe list: that is what is remembered.
  await send(page, "Mira el diff");
  const diff = conv.locator(".ac-chat", { hasText: "git diff HEAD" });
  await expect(diff.locator(".ac-rule")).toHaveText("Bash(git diff:*)");
  await diff.getByRole("button", { name: "Permitir siempre en esta tarea" }).click();
  await expect(conv.locator(".conv-claude", { hasText: "Diff visto." })).toBeVisible();
  await expect
    .poll(async () => (await api.get<Task>(`/api/v1/tasks/${key}`)).allowed_tools)
    .toEqual(expect.arrayContaining(["Bash(npm --version)", "Bash(git diff:*)"]));

  // Covered by the prefix with bounded arguments: no card.
  await send(page, "Y el stat");
  await expect(conv.locator(".conv-claude", { hasText: "Stat visto." })).toBeVisible();
  await expect(conv.getByRole("button", { name: "Permitir una vez" })).toHaveCount(0);
  expect(record(env, s.id).permissions[3]?.response).toMatchObject({ behavior: "allow" });

  // A path outside the workspace after the prefix asks again.
  await send(page, "Escribe el diff fuera");
  const outside = conv.locator(".ac-chat", { hasText: "git diff --output=../x" });
  await expect(outside).toBeVisible();
  await outside.getByRole("button", { name: "Denegar" }).click();
  await expect(conv.locator(".conv-claude", { hasText: "Salida denegada." })).toBeVisible();
  expect(record(env, s.id).permissions[4]?.response).toMatchObject({ behavior: "deny" });
});

test("the runtime switch is refused while a card waits, then goes to the terminal and back to chat", async ({
  page,
  project,
  scenario,
  api,
}) => {
  const p = await project("repo");
  const { key, title } = await startChat(api, scenario, p, [[bash("ls -la", "total 0"), say("Listo.")]]);
  await primarySession(api, key);
  await openTask(page, title);
  const conv = page.locator(".conv");
  const card = conv.locator(".ac-chat", { hasText: "ls -la" });
  await expect(card).toBeVisible({ timeout: 30_000 });

  // A card waits: the switch would cut the turn, so the daemon refuses it.
  const wrap = page.getByRole("group", { name: "Vista de la sesión" });
  const terminal = wrap.getByRole("button", { name: "Terminal", exact: true });
  await terminal.click();
  await page.getByRole("dialog", { name: "Pasar a terminal" }).getByRole("button", { name: "Cambiar" }).click();
  await expect(page.getByRole("alert").filter({ hasText: "Espera a que termine el turno" })).toBeVisible();
  expect((await api.get<Task>(`/api/v1/tasks/${key}`)).chat_mode).toBe(true);

  // Answered and idle: the switch goes through, after a confirmation.
  await card.getByRole("button", { name: "Permitir una vez" }).click();
  await expect(conv.locator(".conv-claude", { hasText: "Listo." })).toBeVisible();
  await expect.poll(() => liveRuntime(api, key)).toBe("stream");
  await expect(terminal).toBeEnabled();
  await terminal.click();
  await page.getByRole("dialog", { name: "Pasar a terminal" }).getByRole("button", { name: "Cambiar" }).click();
  // The switch is per session now (the runtime API): the task's own chat_mode is not touched.
  await expect.poll(() => liveRuntime(api, key), { timeout: 20_000 }).toBe("pty");
  await expect(page.getByRole("alert").filter({ hasText: "Espera a que termine el turno" })).toHaveCount(0);

  // Back to chat from the chat view of the terminal session.
  await wrap.getByRole("button", { name: /^Chat/ }).click();
  await page.getByRole("dialog", { name: "Pasar a chat" }).getByRole("button", { name: "Cambiar" }).click();
  await expect.poll(() => liveRuntime(api, key), { timeout: 20_000 }).toBe("stream");
});

test("a chat session that stopped does not keep reconnecting to its live socket", async ({ page, project, scenario, api }) => {
  const p = await project("repo");
  const { key, title } = await startChat(api, scenario, p, [[say("Hola.")], [{ exit: 0 }]]);
  await primarySession(api, key);
  const sockets: string[] = [];
  page.on("websocket", (ws) => {
    if (ws.url().includes("/live")) sockets.push(ws.url());
  });
  await openTask(page, title);
  const conv = page.locator(".conv");
  await expect(conv.locator(".conv-claude", { hasText: "Hola." })).toBeVisible({ timeout: 30_000 });
  await expect.poll(() => sockets.length).toBeGreaterThan(0);

  // The agent exits on the next message: the session is no longer running.
  await send(page, "Adiós");
  await expect(conv.getByRole("button", { name: "Reabrir" })).toBeVisible();
  const seen = sockets.length;
  await page.waitForTimeout(6000);
  expect(sockets.length).toBe(seen);
});
