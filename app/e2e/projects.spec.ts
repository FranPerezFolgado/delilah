import { expect, test } from "./fixtures";

test.use({ locale: "en-GB" });

test("adding the fixture repo in the UI lists it as a project", async ({ page, fixture }) => {
  const { repo } = fixture();
  const name = `e2e-ui-${Date.now()}`;
  await page.goto("/");
  await page.getByRole("navigation", { name: "Navigation" }).getByRole("button", { name: /^Projects/ }).click();
  await expect(page.getByRole("heading", { name: "Projects" })).toBeVisible();

  await page.getByRole("main").getByRole("button", { name: "Add a project" }).first().click();
  const dialog = page.getByRole("dialog", { name: "Add a project" });
  await dialog.getByPlaceholder("/Users/you/dev/projects/my-project").fill(repo);
  await dialog.getByRole("radio", { name: /^Repo/ }).click();
  await dialog.getByLabel("Name").fill(name);
  await dialog.getByRole("button", { name: "Add", exact: true }).click();
  await expect(dialog).toBeHidden();

  await expect(page.getByText(name).first()).toBeVisible();
  await expect(page.getByRole("navigation", { name: "Navigation" }).getByRole("button", { name, exact: true })).toBeVisible();
});
