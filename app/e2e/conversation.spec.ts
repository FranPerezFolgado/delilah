import { expect, test } from "./fixtures";

// The UI follows the browser language and these specs read the Spanish texts.
test.use({ locale: "es-ES" });

test("confirming the proposal card in a project conversation opens the new task", async ({ page, project, scenario, api }) => {
  const p = await project("repo");
  const title = `Dashboard ${Date.now()}`;
  const brief = "Build a user dashboard with usage charts.";
  // A project conversation has no [fake:] marker; the fake's task there is `@<project name>`.
  scenario(`@${p.name}`, {
    steps: [
      {
        ask: {
          question: title,
          header: "Nueva tarea",
          options: [{ label: "Crear", preview: brief }, "Cambiar algo", "Descartar"],
        },
      },
      { stop: "ok" },
    ],
  });

  await page.goto("/");
  // The sidebar's button for this project.
  await page.getByRole("button", { name: `Nueva tarea con Claude en ${p.name}` }).click();

  await page.getByRole("textbox", { name: "Mensaje" }).fill("I want a user dashboard");
  await page.getByRole("button", { name: "Enviar" }).click();

  const card = page.locator(".conv").getByRole("heading", { name: title });
  await expect(card).toBeVisible({ timeout: 30_000 });
  await expect(page.locator(".conv .ask-preview")).toContainText(brief);
  await page.locator(".conv").getByRole("button", { name: "Crear", exact: true }).click();

  // The answered card is replaced by "Creada" and a link with the new key.
  const link = page.locator(".conv").getByRole("link", { name: /^[A-Z]+-\d+$/ });
  await expect(link).toBeVisible();
  const key = (await link.innerText()).trim();
  await link.click();

  // The new task is open: its title is on screen and the daemon knows it as a backlog task.
  await expect(page.locator(".conv")).toHaveCount(0);
  await expect(page.getByText(title).first()).toBeVisible();
  const task = await api.get<{ key: string; title: string }>(`/api/v1/tasks/${key}`);
  expect(task.title).toBe(title);
});
