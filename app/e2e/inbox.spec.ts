import { expect, test } from "./fixtures";

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

  await page.goto("/");
  await page.getByRole("button", { name: /^Task\b(?!s)/ }).first().click();
  const dialog = page.getByRole("dialog", { name: "New task" });
  await dialog.getByRole("radio", { name: "Free task" }).click();
  await dialog.getByPlaceholder("Try a rates cache in svc-billing").fill(title);
  await dialog.getByRole("combobox", { name: /^Project/ }).selectOption({ label: `${p.name} · repo` });
  await dialog.getByRole("button", { name: "Start" }).click();
  await expect(dialog).toBeHidden();

  await page.getByRole("button", { name: /^Inbox/ }).click();
  await expect(page.getByRole("heading", { name: "Inbox" })).toBeVisible();
  const item = page.locator(".inbox-item", { hasText: "Which colour?" });
  await expect(item.locator(".ask-q")).toContainText("Which colour?");
  await item.getByRole("button", { name: /blue/ }).click();
  await expect(item).toHaveCount(0);

  await page.getByRole("button", { name: /^Tasks/ }).click();
  const card = page.locator(".board .kcard", { hasText: title });
  await expect(card.locator(".kcard-status")).toHaveAttribute("title", "Running");
});
