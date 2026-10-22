import { expect, test } from "./fixtures";

test.use({ locale: "en-GB" });

test("a started task appears on the board and goes from working to waiting", async ({ page, project, scenario }) => {
  scenario("slow-stop", { steps: [{ sleep_ms: 4000 }, { stop: "ok" }] });
  const p = await project("repo");
  const title = `Slow stop ${Date.now()} [fake:slow-stop]`;

  await page.goto("/");
  await page.getByRole("button", { name: /^Task\b(?!s)/ }).first().click();
  const dialog = page.getByRole("dialog", { name: "New task" });
  await dialog.getByRole("radio", { name: "Free task" }).click();
  await dialog.getByPlaceholder("Try a rates cache in svc-billing").fill(title);
  await dialog.getByRole("combobox").selectOption({ label: `${p.name} · repo` });
  await dialog.getByRole("button", { name: "Start" }).click();
  await expect(dialog).toBeHidden();

  await page.getByRole("button", { name: /^Tasks/ }).click();
  const card = page.locator(".board .kcard", { hasText: title });
  await expect(card).toBeVisible();
  const status = card.locator(".kcard-status");
  // Board columns are phases, so the move is the live status of the card.
  await expect(status).toHaveAttribute("title", "Running");
  await expect(status).toHaveAttribute("title", "Waiting for you");
});
