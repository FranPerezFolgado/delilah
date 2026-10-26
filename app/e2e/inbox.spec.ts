import { boardCard, expect, startFreeTask, test } from "./fixtures";

test.use({ locale: "en-GB" });

test("answering an ask card in the inbox clears it and the task works again", async ({ page, project, scenario }) => {
  scenario("ask", {
    steps: [
      { ask: { question: "Which colour?", header: "Colour", options: ["red", "blue"] } },
      { sleep_ms: 3000 },
      { stop: "done" },
    ],
  });
  const p = await project("repo");
  const title = `Ask flow ${Date.now()} [fake:ask]`;

  await startFreeTask(page, p, title);

  const nav = page.getByRole("navigation", { name: "Navigation" });
  await nav.getByRole("button", { name: /^Inbox/ }).click();
  await expect(page.getByRole("heading", { name: "Inbox" })).toBeVisible();
  const item = page.getByRole("option").filter({ hasText: "Which colour?" });
  await item.click();
  const detail = page.locator(".ib-detail");
  await expect(detail).toContainText("Which colour?");
  await detail.getByRole("button", { name: /blue/ }).click();
  await expect(item).toHaveCount(0);

  await nav.getByRole("button", { name: /^Tasks/ }).click();
  const card = await boardCard(page, new RegExp(`Ask flow ${title.match(/\d+/)![0]}`));
  await expect(card.getByText("Working", { exact: true })).toBeVisible();
});

test("Clear notices keeps the blocking question and Undo brings the notices back", async ({ page, project, scenario, api }) => {
  scenario("ask-block", {
    steps: [{ ask: { question: "Keep this one?", header: "Q", options: ["yes", "no"] } }, { sleep_ms: 5000 }, { stop: "done" }],
  });
  const p = await project("repo");
  const stamp = Date.now();
  // The workspaces are worktrees: three tasks cannot share the checkout.
  const run = (title: string, extra: object = {}) =>
    api.post<{ key: string }>("/api/v1/runs", { key: "", title, project: p.name, submodules: [], free: true, in_checkout: false, ...extra });
  const blocker = await run(`Blocker ${stamp}`, { prompt: "Ask [fake:ask-block]" });
  const quiet = [await run(`Quiet A ${stamp}`), await run(`Quiet B ${stamp}`)];
  const waiting = async (key: string) =>
    expect.poll(async () => (await api.get<{ status: { type: string } }>(`/api/v1/tasks/${key}`)).status.type).toBe("waiting-for-user");
  for (const q of quiet) {
    await waiting(q.key);
    // A second Claude that finishes its turn leaves a «turn-done» aviso.
    await api.post(`/api/v1/tasks/${q.key}/sessions`, { kind: "claude", prompt: "Otra" });
  }

  await page.goto("/");
  await page.getByRole("navigation", { name: "Navigation" }).getByRole("button", { name: /^Inbox/ }).click();
  const list = page.getByRole("listbox", { name: "Inbox" });
  const question = list.getByRole("option").filter({ hasText: "Keep this one?" });
  const avisos = quiet.map((q) => list.getByRole("group", { name: q.key }).getByRole("option").filter({ hasText: "Notice" }));
  await expect(question).toBeVisible({ timeout: 30_000 });
  for (const a of avisos) await expect(a.first()).toBeVisible({ timeout: 30_000 });

  await page.getByRole("button", { name: "Clear notices" }).click();
  for (const a of avisos) await expect(a).toHaveCount(0);
  await expect(question).toBeVisible();
  await expect(list.getByRole("group", { name: blocker.key })).toBeVisible();

  await page.getByRole("status").getByRole("button", { name: "Undo" }).click();
  for (const a of avisos) await expect(a.first()).toBeVisible();
  await expect(question).toBeVisible();
});
