import { boardCard, expect, startFreeTask, test } from "./fixtures";

test.use({ locale: "en-GB" });

test("a started task appears on the board and goes from working to waiting", async ({ page, project, scenario }) => {
  scenario("slow-stop", { steps: [{ sleep_ms: 4000 }, { stop: "ok" }] });
  const p = await project("repo");
  const title = `Slow stop ${Date.now()} [fake:slow-stop]`;

  await startFreeTask(page, p, title);

  await page.getByRole("navigation", { name: "Navigation" }).getByRole("button", { name: /^Tasks/ }).click();
  const card = await boardCard(page, title);
  await expect(card).toBeVisible();
  // Board columns are phases, so the move is the live status badge of the card.
  await expect(card.getByText("Working", { exact: true })).toBeVisible();
  await expect(card.getByText("Waiting for you", { exact: true })).toBeVisible();
});
