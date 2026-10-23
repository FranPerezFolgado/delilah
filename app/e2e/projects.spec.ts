import { expect, test } from "./fixtures";

test.use({ locale: "en-GB" });

test("adding the fixture repo in the UI lists it as a project", async ({ page, fixture }) => {
  const { repo } = fixture();
  const name = `e2e-ui-${Date.now()}`;
  await page.goto("/");
  await page.getByRole("button", { name: /Projects/ }).first().click();
  await expect(page.getByRole("heading", { name: "Projects" })).toBeVisible();

  await page.getByPlaceholder("/Users/you/dev/projects/my-project").fill(repo);
  await page.getByLabel("Repo (one repository)").check();
  await page.getByLabel(/^Name \(optional/).fill(name);
  await page.getByRole("button", { name: "Add", exact: true }).click();

  await expect(page.getByText(name).first()).toBeVisible();
  await expect(page.locator(".pgroup .phead", { hasText: name })).toBeVisible();
});
