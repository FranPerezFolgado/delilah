import { expect, test } from "./fixtures";

test("the UI loads against the isolated daemon", async ({ page }) => {
  await page.goto("/");
  await expect(page).toHaveTitle("Delilah");
  await expect(page.locator("#root")).not.toBeEmpty();
});
