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
  // «Nueva tarea» → «Con Claude»: an idea conversation in the project.
  await page.getByRole("button", { name: "Nueva tarea", exact: true }).first().click();
  const dialog = page.getByRole("dialog", { name: "Nueva tarea" });
  await dialog.getByRole("radio", { name: "Con Claude" }).click();
  await dialog.getByRole("combobox", { name: /^Proyecto/ }).selectOption({ label: `${p.name} · repo` });
  await dialog.getByRole("button", { name: "Abrir conversación" }).click();

  await page.getByRole("textbox", { name: "Mensaje" }).fill("I want a user dashboard");
  await page.getByRole("button", { name: /^Enviar/ }).click();

  const card = page.locator(".pc");
  await expect(card.getByRole("heading", { name: title })).toBeVisible({ timeout: 30_000 });
  await expect(card).toContainText(brief);
  await card.getByRole("button", { name: "Crear", exact: true }).click();

  // The answered card is replaced by "Creada" and a link with the new key.
  const link = page.getByRole("log", { name: "Conversación" }).getByRole("link", { name: /^[A-Z]+-\d+$/ });
  await expect(link).toBeVisible();
  const key = (await link.innerText()).trim();
  await link.click();

  // The new task is open: its title is on screen and the daemon knows it as a backlog task.
  await expect(page.locator(".conv")).toHaveCount(0);
  await expect(page.getByText(title).first()).toBeVisible();
  const task = await api.get<{ key: string; title: string }>(`/api/v1/tasks/${key}`);
  expect(task.title).toBe(title);
});
