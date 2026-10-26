import { boardCard, expect, test } from "./fixtures";

// The UI follows the browser language and these specs read the Spanish texts.
test.use({ locale: "es-ES" });

// US3-6: quick open (Mod+P, shortcuts.ts "quick-open") searches the open task's workspace files.
test("quick open finds a workspace file by part of its name and Enter opens it", async ({ page, project, api }) => {
  const p = await project("repo");
  const title = `Quick open ${Date.now()}`;
  // Starting the task creates the workspace whose files quick open searches.
  await api.post("/api/v1/runs", {
    key: "", title, project: p.name, submodules: [], free: true,
  });

  await page.goto("/");
  await page.getByRole("button", { name: /^Tareas/ }).click();
  await (await boardCard(page, title)).click();
  await expect(page.getByRole("tab", { name: "PR", exact: true })).toBeVisible();

  await page.keyboard.press("ControlOrMeta+p");
  const dialog = page.getByRole("dialog", { name: "Abrir archivo" });
  await expect(dialog).toBeVisible();
  await dialog.getByRole("textbox", { name: "Nombre del archivo" }).fill("READM");
  const first = dialog.getByRole("option").first();
  await expect(first).toContainText("README.md");
  await page.keyboard.press("Enter");

  await expect(dialog).toBeHidden();
  await expect(page.getByRole("tab", { name: /README\.md/ })).toBeVisible();
});
