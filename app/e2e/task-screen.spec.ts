import { boardCard, expect, test } from "./fixtures";

test.use({ locale: "en-GB" });

interface Task {
  key: string;
  status: { type: string };
}

async function openTask(page: import("@playwright/test").Page, title: string) {
  await page.goto("/");
  await page.getByRole("navigation", { name: "Navigation" }).getByRole("button", { name: /^Tasks/ }).click();
  await (await boardCard(page, title)).click();
  await expect(page.getByRole("tab", { name: "Task", exact: true })).toBeVisible();
}

const header = (page: import("@playwright/test").Page) => page.getByRole("group", { name: "Task header" });

test("a question is answered from the Task tab, without the Inbox", async ({ page, project, scenario, api }) => {
  scenario("ts-ask", {
    steps: [{ ask: { question: "Which flavour?", header: "Flavour", options: ["vanilla", "mint"] } }, { sleep_ms: 4000 }, { stop: "done" }],
  });
  const p = await project("repo");
  const title = `Task screen ask ${Date.now()}`;
  const task = await api.post<Task>("/api/v1/runs", {
    key: "", title, project: p.name, submodules: [], free: true, prompt: "Ask [fake:ts-ask]",
  });

  await openTask(page, title);
  await page.getByRole("tab", { name: "Task", exact: true }).click();
  const card = page.locator(".ac-inline");
  await expect(card).toContainText("Which flavour?", { timeout: 30_000 });
  await expect(header(page).getByText("Waiting for you", { exact: true })).toBeVisible();
  await card.getByRole("button", { name: /mint/ }).click();

  await expect(header(page).getByText("Working", { exact: true })).toBeVisible();
  await expect(card).toHaveCount(0);
  expect((await api.get<Task>(`/api/v1/tasks/${task.key}`)).status.type).not.toBe("waiting-for-user");
});

test("Mark as done then Undo brings the task back to waiting for you", async ({ page, project, api }) => {
  const p = await project("repo");
  const title = `Task screen done ${Date.now()}`;
  const task = await api.post<Task>("/api/v1/runs", { key: "", title, project: p.name, submodules: [], free: true });
  await expect
    .poll(async () => (await api.get<Task>(`/api/v1/tasks/${task.key}`)).status.type, { timeout: 30_000 })
    .toBe("waiting-for-user");

  await openTask(page, title);
  await expect(header(page).getByText("Waiting for you", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Mark as done" }).click();
  await expect(header(page).getByText("Done", { exact: true })).toBeVisible();

  await page.getByRole("status").getByRole("button", { name: "Undo" }).click();
  await expect(header(page).getByText("Waiting for you", { exact: true })).toBeVisible();
  await expect.poll(async () => (await api.get<Task>(`/api/v1/tasks/${task.key}`)).status.type).toBe("waiting-for-user");
});

test("a very long title keeps the key on one line", async ({ page, project, api }) => {
  const p = await project("repo");
  const title = `Long ${Date.now()} ` + "an extremely long task title that goes on and on ".repeat(8);
  const task = await api.post<Task>("/api/v1/runs", { key: "", title, project: p.name, submodules: [], free: true });

  await page.goto("/");
  await page.getByRole("navigation", { name: "Navigation" }).getByRole("button", { name: /^Tasks/ }).click();
  await (await boardCard(page, new RegExp(`^${task.key} Long`))).click();
  const head = header(page);
  const key = head.getByText(task.key, { exact: true });
  await expect(key).toBeVisible();
  const kb = (await key.boundingBox())!;
  const hb = (await head.boundingBox())!;
  // One line: as tall as a line of text, and inside a header that did not grow.
  expect(kb.height).toBeLessThan(26);
  expect(hb.height).toBeLessThan(80);
  const tb = (await head.getByRole("heading").boundingBox())!;
  expect(tb.height).toBeLessThan(40);
  expect(tb.x + tb.width).toBeLessThanOrEqual(hb.x + hb.width + 1);
});
