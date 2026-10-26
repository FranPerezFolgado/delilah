// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { ApiError, DEFAULT_PROJECT_SETTINGS, api, type Project, type Task } from "../api";
import { setLanguage } from "../i18n";
import { BacklogTaskView } from "./BacklogTaskView";
import { ToastHost, clearToasts } from "./ui";

vi.mock("./WherePicker", async (orig) => ({ ...(await orig<typeof import("./WherePicker")>()), WherePicker: () => null }));

const web: Project = { name: "web", path: "/w/web", kind: "repo", workflow: "feature", settings: { ...DEFAULT_PROJECT_SETTINGS, workflow: { kind: "phased", plan: "/p", implement: "/i", review: "/r" } } };
const task: Task = {
  key: "LIBRE-9",
  created_at: new Date(Date.now() - 26 * 3_600_000).toISOString(),
  finished_at: null,
  title: "Algo",
  project: "web",
  workflow: "feature",
  phase: null,
  status: { type: "queued" },
  saving_mode: false,
  title_source: "manual",
  previous_title: null,
  branch_error: null,
  autopilot: null,
  handoff: null,
};

beforeEach(() => setLanguage("es"));
afterEach(() => {
  cleanup();
  clearToasts();
  vi.restoreAllMocks();
});

const mount = (t: Task, extra: { onOpen?: (k: string) => void; onChanged?: () => void } = {}) =>
  render(
    <>
      <BacklogTaskView task={t} projects={[web]} onChanged={extra.onChanged ?? (() => {})} onOpen={extra.onOpen} />
      <ToastHost />
    </>,
  );
const btn = (c: HTMLElement, name: string) => Array.from(c.querySelectorAll("button")).find((b) => b.textContent?.trim() === name) as HTMLButtonElement;
const toggle = (c: HTMLElement, name: string) => Array.from(c.querySelectorAll<HTMLButtonElement>("button.toggle")).find((b) => b.textContent === name)!;

describe("BacklogTaskView", () => {
  it("shows key, title, Backlog badge and the creation date relatively", () => {
    const { container } = mount(task);
    expect(container.querySelector(".bt-key")?.textContent).toBe("LIBRE-9");
    expect(container.querySelector(".bt-title")?.textContent).toBe("Algo");
    expect(container.querySelector(".badge")?.textContent).toBe("Backlog");
    expect(container.querySelector(".bt-when")?.textContent).toBe("Creada ayer");
  });

  it("Iniciar runs the task, stays on screen with the started state and a toast with Abrir", async () => {
    const run = vi.spyOn(api, "run").mockResolvedValue({ ...task, status: { type: "running" } });
    const onOpen = vi.fn();
    const onChanged = vi.fn();
    const { container } = mount(task, { onOpen, onChanged });
    fireEvent.click(btn(container, "Iniciar"));
    await waitFor(() => expect(run).toHaveBeenCalled());
    expect(run.mock.calls[0][0]).toMatchObject({ key: "LIBRE-9", project: "web", in_checkout: true });
    await waitFor(() => expect(container.querySelector(".badge")?.textContent).toBe("Trabajando"));
    expect(onChanged).not.toHaveBeenCalled();
    const status = container.querySelector(".toast-host .toast")!;
    expect(status.textContent).toContain("LIBRE-9 iniciada");
    fireEvent.click(btn(status as HTMLElement, "Abrir"));
    expect(onOpen).toHaveBeenCalledWith("LIBRE-9");
  });

  it("a dirty checkout shows one human error with «Iniciar en un worktree»", async () => {
    const run = vi
      .spyOn(api, "run")
      .mockRejectedValueOnce(new ApiError("anything the daemon words", 409, false, "checkout-dirty"))
      .mockResolvedValue({ ...task, status: { type: "running" } });
    const { container } = mount(task);
    fireEvent.click(btn(container, "Iniciar"));
    await waitFor(() => expect(container.querySelector(".bt-refused")).not.toBeNull());
    expect(container.querySelector(".bt-refused")?.textContent).toContain("Tu checkout tiene cambios sin guardar");
    fireEvent.click(btn(container, "Iniciar en un worktree"));
    await waitFor(() => expect(run).toHaveBeenCalledTimes(2));
    expect(run.mock.calls[1][0].in_checkout).toBe(false);
  });

  it("Eliminar… asks «¿Eliminar LIBRE-9?» and deletes on confirm", async () => {
    const del = vi.spyOn(api, "deleteTask").mockResolvedValue(null as never);
    const onChanged = vi.fn();
    const { container } = mount(task, { onChanged });
    fireEvent.click(btn(container, "Eliminar…"));
    const dialog = container.ownerDocument.querySelector("[role=dialog]")!;
    expect(dialog.textContent).toContain("¿Eliminar LIBRE-9?");
    expect(del).not.toHaveBeenCalled();
    fireEvent.click(btn(dialog as HTMLElement, "Eliminar"));
    await waitFor(() => expect(del).toHaveBeenCalledWith("LIBRE-9"));
    await waitFor(() => expect(onChanged).toHaveBeenCalled());
  });

  it("Guardar is enabled only with changes and saves them", async () => {
    const update = vi.spyOn(api, "updateTask").mockResolvedValue(task);
    const { container } = mount(task);
    expect(btn(container, "Guardar").disabled).toBe(true);
    fireEvent.change(container.querySelector("input.input")!, { target: { value: "Otro" } });
    fireEvent.click(btn(container, "Guardar"));
    await waitFor(() => expect(update).toHaveBeenCalled());
    expect(update.mock.calls[0][1]).toMatchObject({ title: "Otro" });
  });
});

describe("BacklogTaskView autopilot", () => {
  it("says why it is hidden: custom prompt vs project not phased", () => {
    const prompt = mount({ ...task, workflow: "prompt" });
    expect(prompt.container.textContent).toContain("no con un prompt propio");
    prompt.unmount();
    const other = render(<BacklogTaskView task={task} projects={[{ ...web, settings: DEFAULT_PROJECT_SETTINGS }]} onChanged={() => {}} />);
    expect(other.container.textContent).toContain("flujo por fases");
  });

  it("is offered for a task whose workflow is \"project\"", () => {
    const { container } = mount({ ...task, workflow: "project" });
    expect(toggle(container, "Autopiloto")).toBeDefined();
  });

  it("turning it on asks first, then ticks and disables Ahorro, and saves both", async () => {
    const update = vi.spyOn(api, "updateTask").mockResolvedValue(task);
    const { container } = mount(task);
    fireEvent.click(toggle(container, "Autopiloto"));
    expect(container.ownerDocument.querySelector("[role=dialog]")?.textContent).toContain("Activar autopiloto");
    expect(toggle(container, "Autopiloto").getAttribute("aria-pressed")).toBe("false");
    await act(async () => {
      fireEvent.click(btn(container.ownerDocument.querySelector("[role=dialog]") as HTMLElement, "Activar"));
    });
    const saving = toggle(container, "Ahorro");
    expect(saving.getAttribute("aria-pressed")).toBe("true");
    expect(saving.disabled).toBe(true);
    fireEvent.click(btn(container, "Guardar"));
    await waitFor(() => expect(update).toHaveBeenCalled());
    expect(update.mock.calls[0][1]).toMatchObject({ autopilot: true, saving_mode: true });
  });

  it("turning it off re-enables Ahorro, still on", () => {
    const { container } = mount({ ...task, saving_mode: true, autopilot: { state: "running", since: "2026-01-01T00:00:00Z" } });
    expect(toggle(container, "Ahorro").disabled).toBe(true);
    fireEvent.click(toggle(container, "Autopiloto"));
    expect(toggle(container, "Ahorro").disabled).toBe(false);
    expect(toggle(container, "Ahorro").getAttribute("aria-pressed")).toBe("true");
  });

  it("words Ahorro as one fresh Claude session per phase", () => {
    const { container } = mount(task);
    expect(toggle(container, "Ahorro").title).toBe("Una sesión de Claude nueva por fase, para gastar menos contexto");
  });

  it("is hidden, with a reason, for a task outside the project workflow", () => {
    const { container } = mount({ ...task, workflow: "prompt" });
    expect(container.querySelectorAll("button.toggle")).toHaveLength(0);
    expect(container.textContent).toContain("no con un prompt propio");
  });

  it("is hidden when the project's workflow is not phased", () => {
    const { container } = render(
      <BacklogTaskView task={task} projects={[{ ...web, settings: DEFAULT_PROJECT_SETTINGS }]} onChanged={() => {}} />,
    );
    expect(container.querySelectorAll("button.toggle")).toHaveLength(0);
    expect(container.textContent).toContain("solo están disponibles con un flujo por fases");
  });
});

describe("BacklogTaskView start", () => {
  beforeEach(() => vi.spyOn(api, "projectSkills").mockResolvedValue([]));
  const startSelect = (c: HTMLElement) => c.querySelector('select[aria-label="Al empezar"]') as HTMLSelectElement;

  it("shows the stored message and the project's own start for a plain task", () => {
    const a = mount({ ...task, workflow: "prompt", start_message: "/mine" });
    expect(startSelect(a.container).value).toBe("prompt");
    expect((a.container.querySelector('input[aria-label="Comando"]') as HTMLInputElement).value).toBe("/mine");
    a.unmount();
    const b = mount({ ...task, workflow: "project", start_message: null });
    expect(startSelect(b.container).value).toBe("project");
    expect(b.container.querySelector(".start-resolved")!.textContent).toBe("/p");
  });

  it("saves a changed start with PATCH workflow and start_message", async () => {
    const update = vi.spyOn(api, "updateTask").mockResolvedValue(task);
    const { container } = mount({ ...task, workflow: "project" });
    expect(btn(container, "Guardar").disabled).toBe(true);
    fireEvent.change(startSelect(container), { target: { value: "prompt" } });
    fireEvent.change(container.querySelector('input[aria-label="Comando"]')!, { target: { value: "/fix {ticket}" } });
    fireEvent.click(btn(container, "Guardar"));
    await waitFor(() => expect(update).toHaveBeenCalled());
    expect(update.mock.calls[0][1]).toMatchObject({ workflow: "prompt", start_message: "/fix {ticket}" });
  });

  it("disables Iniciar and Guardar while the custom message is blank", () => {
    const { container } = mount({ ...task, workflow: "project" });
    fireEvent.change(startSelect(container), { target: { value: "prompt" } });
    expect(btn(container, "Iniciar").disabled).toBe(true);
    expect(btn(container, "Guardar").disabled).toBe(true);
    fireEvent.change(container.querySelector('input[aria-label="Comando"]')!, { target: { value: "/fix" } });
    expect(btn(container, "Iniciar").disabled).toBe(false);
    expect(btn(container, "Guardar").disabled).toBe(false);
  });

  it("Nada saves workflow none and hides autopilot", async () => {
    const update = vi.spyOn(api, "updateTask").mockResolvedValue(task);
    const { container } = mount({ ...task, workflow: "project" });
    fireEvent.change(startSelect(container), { target: { value: "none" } });
    expect(container.querySelectorAll("button.toggle")).toHaveLength(0);
    fireEvent.click(btn(container, "Guardar"));
    await waitFor(() => expect(update).toHaveBeenCalled());
    expect(update.mock.calls[0][1]).toMatchObject({ workflow: "none", start_message: "" });
  });

  it("starts with the chosen start", async () => {
    const run = vi.spyOn(api, "run").mockResolvedValue({ ...task, status: { type: "running" } });
    const { container } = mount({ ...task, workflow: "prompt", start_message: "/mine" });
    fireEvent.click(btn(container, "Iniciar"));
    await waitFor(() => expect(run).toHaveBeenCalled());
    expect(run.mock.calls[0][0]).toMatchObject({ workflow: "prompt", prompt: "/mine" });
  });

  it("shows template errors inline", async () => {
    vi.spyOn(api, "updateTask").mockRejectedValue(new ApiError("invalid", 422, false, undefined, undefined, [{ field: "workflow.start.template", code: "x", message: "placeholder desconocido" }]));
    const { container } = mount({ ...task, workflow: "prompt", start_message: "/mine" });
    fireEvent.change(container.querySelector('input[aria-label="Comando"]')!, { target: { value: "/x {nope}" } });
    fireEvent.click(btn(container, "Guardar"));
    await waitFor(() => expect(container.querySelector('[data-field="workflow.start.template"]')).not.toBeNull());
  });
});
