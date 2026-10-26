import type { Page } from "@playwright/test";
import { boardCard, expect, test } from "./fixtures";

test.use({ locale: "en-GB" });

const nav = (page: Page) => page.getByRole("navigation", { name: "Navigation" });

/** The page itself never scrolls sideways (a board's own column scroller does not count). */
async function noPageScroll(page: Page, what: string) {
  const [scroll, inner] = await page.evaluate(() => [document.scrollingElement!.scrollWidth, window.innerWidth]);
  expect(scroll, `${what}: scrollWidth ${scroll} > innerWidth ${inner}`).toBeLessThanOrEqual(inner);
}

test("at 1024 no screen scrolls sideways and the details are overlays; at 1280 the sidebar is a rail", async ({
  page, project, scenario, api, env,
}) => {
  scenario("w-ask", { steps: [{ ask: { question: "Wide enough?", header: "W", options: ["yes", "no"] } }, { sleep_ms: 5000 }, { stop: "done" }] });
  const p = await project("repo");
  const title = `Widths ${Date.now()}`;
  await api.post("/api/v1/runs", { key: "", title, project: p.name, submodules: [], free: true, prompt: "Ask [fake:w-ask]" });
  await fetch(`${env.api}/api/v1/jira`, {
    method: "PUT",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ mode: "sample" }),
  });

  // The sidebar is a rail below 1300 and complete above it.
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/");
  await expect(nav(page).getByRole("button", { name: "Expand sidebar" })).toBeVisible();
  await noPageScroll(page, "1280");
  await page.setViewportSize({ width: 1440, height: 900 });
  await expect(nav(page).getByRole("button", { name: "Expand sidebar" })).toHaveCount(0);

  await page.setViewportSize({ width: 1024, height: 768 });
  await page.goto("/");
  await expect(nav(page).getByRole("button", { name: /^Inbox/ })).toBeVisible();

  // Inbox: the list alone until a row is chosen, then the detail is a dialog.
  await nav(page).getByRole("button", { name: /^Inbox/ }).click();
  const row = page.getByRole("option").filter({ hasText: "Wide enough?" });
  await expect(row).toBeVisible({ timeout: 30_000 });
  await noPageScroll(page, "Inbox");
  await expect(page.getByRole("dialog", { name: "Notice detail" })).toHaveCount(0);
  await row.click();
  const detail = page.getByRole("dialog", { name: "Notice detail" });
  await expect(detail).toBeVisible();
  await noPageScroll(page, "Inbox detail");
  await detail.getByRole("button", { name: "Close" }).click();
  await expect(detail).toHaveCount(0);

  // Board.
  await nav(page).getByRole("button", { name: /^Tasks/ }).click();
  await expect(page.getByRole("heading", { name: "Tasks" })).toBeVisible();
  await noPageScroll(page, "Board");

  // Task screen.
  await (await boardCard(page, title)).click();
  await expect(page.getByRole("tab", { name: "Task", exact: true })).toBeVisible();
  await noPageScroll(page, "Task");

  // Jira: the ticket opens as a drawer.
  await nav(page).getByRole("button", { name: /^Jira/ }).click();
  const ticket = page.getByText("PROJ-1242", { exact: true }).first();
  await expect(ticket).toBeVisible();
  await noPageScroll(page, "Jira");
  await ticket.click();
  await expect(page.getByRole("dialog", { name: "PROJ-1242" })).toBeVisible();
  await noPageScroll(page, "Jira drawer");
  // The drawer's scrim closes it.
  await page.locator(".jv-scrim").click({ position: { x: 10, y: 300 } });
  await expect(page.getByRole("dialog", { name: "PROJ-1242" })).toHaveCount(0);

  // Projects, the project's screen, Settings.
  await nav(page).getByRole("button", { name: /^Projects/ }).click();
  await expect(page.getByRole("heading", { name: "Projects" })).toBeVisible();
  await noPageScroll(page, "Projects");
  await nav(page).getByRole("button", { name: p.name, exact: true }).click();
  await expect(page.getByRole("tab", { name: "Project", exact: true })).toBeVisible();
  await noPageScroll(page, "Project");
  await nav(page).getByRole("button", { name: /^Settings/ }).click();
  await expect(page.getByRole("heading", { name: "Settings" })).toBeVisible();
  await noPageScroll(page, "Settings");
});
