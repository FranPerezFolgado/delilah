// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { DEFAULT_PROJECT_SETTINGS, api, type Project, type Task } from "../api";
import { setLanguage } from "../i18n";
import { BacklogTaskView } from "./BacklogTaskView";
import { ToastHost, clearToasts } from "./ui";

vi.mock("./WherePicker", async (orig) => ({ ...(await orig<typeof import("./WherePicker")>()), WherePicker: () => null }));

const web: Project = { name: "web", path: "/w/web", kind: "repo", workflow: "feature", settings: DEFAULT_PROJECT_SETTINGS };
const task: Task = {
  key: "LIBRE-9",
  created_at: new Date(Date.now() - 26 * 3_600_000).toISOString(),
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
    const status = container.querySelector("[role=status].toast")!;
    expect(status.textContent).toContain("LIBRE-9 iniciada");
    fireEvent.click(btn(status as HTMLElement, "Abrir"));
    expect(onOpen).toHaveBeenCalledWith("LIBRE-9");
  });

  it("a dirty checkout shows one human error with «Iniciar en un worktree»", async () => {
    const run = vi
      .spyOn(api, "run")
      .mockRejectedValueOnce(new Error("el checkout tiene cambios sin confirmar"))
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

  it("is hidden for a task outside the spec workflow", () => {
    const { container } = mount({ ...task, workflow: "prompt" });
    expect(Array.from(container.querySelectorAll("button.toggle")).map((b) => b.textContent)).toEqual(["Ahorro"]);
  });
});
