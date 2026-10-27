// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { DEFAULT_PROJECT_SETTINGS, api, type Task } from "../api";
import { setLanguage } from "../i18n";
import { TaskHeader, type TaskHeaderProps } from "./TaskHeader";

vi.mock("../live", () => ({ on: () => () => {} }));

let width = 1200;
beforeEach(() => {
  setLanguage("es");
  width = 1200;
  vi.spyOn(api, "projects").mockResolvedValue([
    { name: "app", path: "/x", kind: "repo", workflow: "feature", settings: DEFAULT_PROJECT_SETTINGS, preview_command: "npm run dev" },
  ]);
  vi.spyOn(api, "getPreview").mockResolvedValue(null);
  vi.stubGlobal(
    "ResizeObserver",
    class {
      cb: ResizeObserverCallback;
      constructor(cb: ResizeObserverCallback) {
        this.cb = cb;
      }
      observe() {
        this.cb([{ contentRect: { width } } as ResizeObserverEntry], this as unknown as ResizeObserver);
      }
      disconnect() {}
    },
  );
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

const task = (over: Partial<Task> = {}): Task =>
  ({
    key: "DEL-12",
    title: "Un título muy largo que no cabe",
    project: "app",
    workflow: "feature",
    phase: "plan",
    status: { type: "running" },
    saving_mode: false,
    autopilot: null,
    handoff: null,
    title_source: "initial",
    previous_title: null,
    branch_error: null,
    ...over,
  }) as Task;

function setup(over: Partial<TaskHeaderProps> = {}, t: Partial<Task> = {}) {
  const props: TaskHeaderProps = {
    task: task(t),
    phase: { column: "plan", step: 2 },
    owner: "DEL-12",
    hasWorkspace: true,
    showTree: false,
    onToggleTree: vi.fn(),
    onAutopilot: vi.fn(),
    onSavingMode: vi.fn(),
    onChanged: vi.fn(),
    onError: vi.fn(),
    ...over,
  };
  return { props, ...render(<TaskHeader {...props} />) };
}

it("shows the key (nowrap, mono) and the full title in its tooltip", () => {
  const { container } = setup();
  const key = container.querySelector(".th-key")!;
  expect(key.textContent).toBe("DEL-12");
  expect(key.className).toContain("mono");
  const h1 = container.querySelector("h1")!;
  expect(h1.textContent).toBe("Un título muy largo que no cabe");
  expect(h1.getAttribute("title")).toContain("Un título muy largo que no cabe");
});

it("renames on click + Enter, and refuses an empty title", async () => {
  const update = vi.spyOn(api, "updateTask").mockResolvedValue(task());
  const { container, props } = setup();
  fireEvent.click(container.querySelector("h1")!);
  const input = screen.getByLabelText("Título de la tarea");
  fireEvent.change(input, { target: { value: "  Nuevo  " } });
  fireEvent.keyDown(input, { key: "Enter" });
  expect(update).toHaveBeenCalledWith("DEL-12", { title: "Nuevo" });
  await waitFor(() => expect(props.onChanged).toHaveBeenCalled());

  fireEvent.click(container.querySelector("h1")!);
  fireEvent.change(screen.getByLabelText("Título de la tarea"), { target: { value: "   " } });
  fireEvent.keyDown(screen.getByLabelText("Título de la tarea"), { key: "Enter" });
  expect(update).toHaveBeenCalledTimes(1);
});

it("offers Antes: <old> · Restaurar after a rename", () => {
  const update = vi.spyOn(api, "updateTask").mockResolvedValue(task());
  const { container } = setup({}, { previous_title: "Viejo" });
  expect(container.querySelector(".th-before")!.textContent).toContain("Antes: Viejo");
  fireEvent.click(screen.getByText("Restaurar"));
  expect(update).toHaveBeenCalledWith("DEL-12", { title: "Viejo" });
});

it("shows the status and the phase bars; the phase popover lists the 4 phases and sets one", async () => {
  const set = vi.spyOn(api, "setPhase").mockResolvedValue(task());
  const { container, props } = setup();
  expect(container.querySelector(".badge")!.textContent).toBe("Trabajando");
  fireEvent.click(screen.getByRole("button", { name: "Cambiar de fase" }));
  const items = screen.getAllByRole("menuitem").map((i) => i.textContent);
  expect(items).toEqual(["Especificación", "✓ Plan", "Implementación", "Revisión"]);
  fireEvent.click(screen.getByRole("menuitem", { name: "Revisión" }));
  expect(set).toHaveBeenCalledWith("DEL-12", "review");
  await waitFor(() => expect(props.onChanged).toHaveBeenCalled());
});

it("renders the session switch slot, the autopilot controls and the tree toggle", () => {
  const { props } = setup({ sessionSwitch: <span>SWITCH</span> });
  screen.getByText("SWITCH");
  screen.getByRole("button", { name: "Autopiloto" });
  fireEvent.click(screen.getByRole("button", { name: "Ahorro" }));
  expect(props.onSavingMode).toHaveBeenCalledWith(true);
  const tree = screen.getByRole("button", { name: "Árbol de archivos" });
  expect(tree.getAttribute("aria-pressed")).toBe("false");
  fireEvent.click(tree);
  expect(props.onToggleTree).toHaveBeenCalled();
});

it("hides the tree toggle without a workspace", () => {
  setup({ hasWorkspace: false });
  expect(screen.queryByRole("button", { name: "Árbol de archivos" })).toBeNull();
});

it("shows the Preview control", async () => {
  setup();
  await screen.findByRole("button", { name: "Preview" });
});

it("a done task shows Hecha, the PR chip and the date, with no bars or autopilot", () => {
  const { container } = setup({ pr: { number: 7 }, doneAt: "2020-01-01T00:00:00Z" }, { status: { type: "done" } });
  expect(container.querySelector(".badge")!.textContent).toBe("Hecha");
  expect(container.querySelector(".prchip")!.textContent).toContain("#7");
  expect(container.querySelector(".th-when")!.textContent).not.toBe("");
  expect(container.querySelector(".phasebars")).toBeNull();
  expect(screen.queryByRole("button", { name: "Autopiloto" })).toBeNull();
});

it("a setup-failed task shows Sin iniciar", () => {
  const { container } = setup({ phase: null }, { status: { type: "setup-failed", reason: "x" } });
  expect(container.querySelector(".phasebars")!.getAttribute("aria-label")).toBe("Sin iniciar");
});

it("is compact at 960 or less: bars only, icon-only toggles and Preview", async () => {
  width = 960;
  const { container } = setup();
  expect(container.querySelector(".th.compact")).not.toBeNull();
  expect(container.querySelector(".phasebars .lbl")).toBeNull();
  expect(screen.getByRole("button", { name: "Autopiloto" }).textContent).toBe("");
  expect((await screen.findByRole("button", { name: "Preview" })).textContent).toBe("");
});

it("is full above 960", () => {
  width = 961;
  const { container } = setup();
  expect(container.querySelector(".th.compact")).toBeNull();
  expect(container.querySelector(".phasebars .lbl")).not.toBeNull();
});

it("a parent task shows its subtask progress instead of the status", () => {
  const { container } = setup({}, { kind: "parent_task", subtask_progress: { merged: 2, counted: 3, dropped: 0, ready: false } });
  expect(container.querySelector(".th-progress")!.textContent).toBe("2 de 3 subtareas integradas");
  expect(container.textContent).not.toContain("Trabajando");
});

it("a subtask links to its parent task", () => {
  const { container, props } = setup({ onOpenTask: vi.fn() }, { kind: "subtask", parent_task: "DEL-1" });
  fireEvent.click(container.querySelector(".th-parent button")!);
  expect(props.onOpenTask).toHaveBeenCalledWith("DEL-1");
  expect(container.textContent).toContain("Trabajando");
});

it("offers to push the parent task's branch again only when it is missing", async () => {
  const push = vi.spyOn(api, "pushBranch").mockResolvedValue(null);
  const none = setup({}, { kind: "parent_task" });
  expect(none.queryByText("Volver a subir la rama")).toBeNull();
  none.unmount();
  const { getByText, props } = setup({ branchMissing: true }, { kind: "subtask", parent_task: "DEL-1" });
  fireEvent.click(getByText("Volver a subir la rama"));
  expect(push).toHaveBeenCalledWith("DEL-1");
  await waitFor(() => expect(props.onChanged).toHaveBeenCalled());
});

it("offers «Convertir en tarea padre» on a started normal task and shows the refusal", async () => {
  const make = vi.spyOn(api, "makeParentTask").mockResolvedValueOnce(task({ kind: "parent_task" })).mockRejectedValueOnce(new Error("Tiene un PR abierto"));
  const { props } = setup({}, { kind: "normal" });
  fireEvent.click(screen.getByRole("button", { name: "Tarea padre" }));
  fireEvent.click(screen.getByRole("menuitem", { name: "Convertir en tarea padre" }));
  expect(make).toHaveBeenCalledWith("DEL-12");
  await waitFor(() => expect(props.onChanged).toHaveBeenCalled());
  fireEvent.click(screen.getByRole("button", { name: "Tarea padre" }));
  fireEvent.click(screen.getByRole("menuitem", { name: "Convertir en tarea padre" }));
  await waitFor(() => expect(props.onError).toHaveBeenCalledWith("Tiene un PR abierto"));
});

it("has no parent task menu on a parent task or a subtask", () => {
  setup({}, { kind: "parent_task" });
  expect(screen.queryByRole("button", { name: "Tarea padre" })).toBeNull();
  cleanup();
  setup({}, { kind: "subtask", parent_task: "DEL-1" });
  expect(screen.queryByRole("button", { name: "Tarea padre" })).toBeNull();
});
