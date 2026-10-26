import { existsSync, writeFileSync } from "node:fs";
import path from "node:path";
import { expect, test } from "./fixtures";

test.use({ locale: "en-GB", viewport: { width: 1440, height: 900 } });

interface Task {
  key: string;
  title: string;
  in_checkout?: boolean | null;
  status: { type: string };
}

async function pollTask(api: import("./fixtures").Api, title: string): Promise<Task> {
  let found: Task | undefined;
  await expect
    .poll(async () => {
      // The newest one: a refused start can leave an errored record with the same title.
      found = (await api.get<Task[]>("/api/v1/tasks")).filter((t) => t.title === title).at(-1);
      return !!found;
    })
    .toBe(true);
  return found!;
}

test("the dialog box is the same for every origin and with advanced options open", async ({ page, project }) => {
  await project("repo");
  await page.goto("/");
  await page.getByRole("button", { name: "New task", exact: true }).first().click();
  const dialog = page.getByRole("dialog", { name: "New task" });
  await expect(dialog).toBeVisible();

  const boxes: { x: number; y: number; width: number; height: number }[] = [];
  const measure = async () => {
    const box = (await dialog.boundingBox())!;
    boxes.push(box);
    // «Start» is always on screen.
    const start = (await dialog.getByRole("button", { name: /^(Start|Open conversation)$/ }).boundingBox())!;
    const vp = page.viewportSize()!;
    expect(start.y + start.height).toBeLessThanOrEqual(vp.height);
    expect(start.x + start.width).toBeLessThanOrEqual(vp.width);
  };
  for (const origin of ["Jira ticket", "Free task", "With Claude"]) {
    await dialog.getByRole("radio", { name: origin }).click();
    await expect(dialog.getByRole("radio", { name: origin })).toHaveAttribute("aria-checked", "true");
    await measure();
  }
  await dialog.getByRole("radio", { name: "Free task" }).click();
  await dialog.getByText("Advanced options").click();
  await expect(dialog.getByRole("group", { name: "View" }).or(dialog.getByRole("radiogroup", { name: "View" }))).toBeVisible();
  await measure();

  for (const b of boxes.slice(1)) {
    expect(b.width).toBeCloseTo(boxes[0].width, 0);
    expect(b.height).toBeCloseTo(boxes[0].height, 0);
    expect(b.x).toBeCloseTo(boxes[0].x, 0);
    expect(b.y).toBeCloseTo(boxes[0].y, 0);
  }
});

test("a free task with no workspace choice runs in the project's checkout", async ({ page, project, api, env }) => {
  const p = await project("repo");
  const title = `Checkout default ${Date.now()}`;
  await page.goto("/");
  await page.getByRole("button", { name: "New task", exact: true }).first().click();
  const dialog = page.getByRole("dialog", { name: "New task" });
  await dialog.getByRole("radio", { name: "Free task" }).click();
  // The checkout is the labelled default.
  await expect(dialog.getByRole("radio", { name: /In your checkout.*default/ })).toHaveAttribute("aria-checked", "true");
  await dialog.getByPlaceholder("Try a rates cache in svc-billing").fill(title);
  await dialog.getByRole("combobox", { name: /^Project/ }).selectOption({ label: `${p.name} · repo` });
  await dialog.getByRole("button", { name: "Start", exact: true }).click();
  await expect(dialog).toBeHidden();

  const task = await pollTask(api, title);
  await expect.poll(async () => (await api.get<Task>(`/api/v1/tasks/${task.key}`)).status.type, { timeout: 30_000 }).toBe("waiting-for-user");
  // The checkout is the project's own folder: no worktree was made for the task.
  expect(existsSync(path.join(env.home, "workspaces", task.key))).toBe(false);
});

test("a dirty checkout refuses «Start» and «Start in a worktree» starts it isolated", async ({ page, project, api, env }) => {
  const p = await project("repo");
  writeFileSync(path.join(p.path, "uncommitted.txt"), "not committed\n");
  const title = `Dirty checkout ${Date.now()}`;
  await page.goto("/");
  await page.getByRole("button", { name: "New task", exact: true }).first().click();
  const dialog = page.getByRole("dialog", { name: "New task" });
  await dialog.getByRole("radio", { name: "Free task" }).click();
  await dialog.getByPlaceholder("Try a rates cache in svc-billing").fill(title);
  await dialog.getByRole("combobox", { name: /^Project/ }).selectOption({ label: `${p.name} · repo` });
  await dialog.getByRole("button", { name: "Start", exact: true }).click();

  const alert = dialog.getByRole("alert").filter({ hasText: "unsaved changes" });
  await expect(alert).toBeVisible();
  await alert.getByRole("button", { name: "Start in a worktree" }).click();
  await expect(dialog).toBeHidden();

  const task = await pollTask(api, title);
  await expect.poll(() => existsSync(path.join(env.home, "workspaces", task.key)), { timeout: 30_000 }).toBe(true);
});
