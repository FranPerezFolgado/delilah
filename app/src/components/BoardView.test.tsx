// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { setLanguage } from "../i18n";
import { api, type Project, type SpecSummary, type Task } from "../api";
import { BoardView } from "./BoardView";

beforeEach(() => {
  setLanguage("es");
  vi.spyOn(api, "preference").mockResolvedValue(null as never);
  vi.spyOn(api, "jira").mockResolvedValue({ settings: null });
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const task = (key: string, over: Partial<Task> = {}): Task =>
  ({ key, title: `Title ${key}`, project: "alpha", phase: "plan", status: { type: "running" }, workflow: "w", saving_mode: false, ...over }) as Task;
const proj = (name: string) => ({ name }) as Project;
const specs: Record<string, SpecSummary> = {};

const setup = (tasks: Task[], extra: Record<string, unknown> = {}) => {
  const onOpen = vi.fn();
  const onNewTask = vi.fn();
  render(<BoardView tasks={tasks} specs={specs} projects={[proj("alpha"), proj("beta")]} onOpen={onOpen} onNewTask={onNewTask} {...extra} />);
  return { onOpen, onNewTask };
};

it("renders Backlog, four phases and Hecha", () => {
  setup([task("A-1")]);
  for (const n of ["Backlog", "Especificación", "Plan", "Implementación", "Revisión", "Hecha"]) {
    expect(screen.getByRole("region", { name: n })).not.toBeNull();
  }
});

it("folds empty phase columns", () => {
  setup([task("A-1")]);
  expect(screen.getByRole("region", { name: "Plan" }).className).not.toContain("folded");
  expect(screen.getByRole("region", { name: "Revisión" }).className).toContain("folded");
  expect(screen.getByRole("region", { name: "Backlog" }).className).not.toContain("folded");
});

it("a card is a link with key and title, status text and mini bars", () => {
  const { onOpen } = setup([task("A-1")]);
  const card = screen.getByRole("link", { name: "A-1 Title A-1" });
  expect(within(card).getByText("Trabajando")).not.toBeNull();
  expect(card.querySelector(".bars.mini")).not.toBeNull();
  fireEvent.click(card);
  expect(onOpen).toHaveBeenCalledWith("A-1");
});

it("Backlog and Hecha cards have no phase bars", () => {
  setup([task("A-1", { status: { type: "queued" } }), task("A-2", { status: { type: "done" } })]);
  expect(screen.getByRole("link", { name: "A-1 Title A-1" }).querySelector(".bars")).toBeNull();
  expect(screen.getByRole("link", { name: "A-2 Title A-2" }).querySelector(".bars")).toBeNull();
});

it("filters by project", () => {
  setup([task("A-1"), task("B-1", { project: "beta" })]);
  fireEvent.click(screen.getByRole("radio", { name: "beta" }));
  expect(screen.queryByRole("link", { name: "A-1 Title A-1" })).toBeNull();
  expect(screen.getByRole("link", { name: "B-1 Title B-1" })).not.toBeNull();
});

it("statusFilter keeps only that status and can be cleared", () => {
  const onClear = vi.fn();
  setup([task("A-1"), task("A-2", { status: { type: "waiting-for-user" } })], { statusFilter: "wait", onClearStatusFilter: onClear });
  expect(screen.queryByRole("link", { name: "A-1 Title A-1" })).toBeNull();
  expect(screen.getByRole("link", { name: "A-2 Title A-2" })).not.toBeNull();
  fireEvent.click(screen.getByRole("button", { name: /Estado/ }));
  expect(onClear).toHaveBeenCalled();
});

it("Nueva tarea is primary", () => {
  const { onNewTask } = setup([task("A-1")]);
  const b = screen.getByRole("button", { name: "Nueva tarea" });
  expect(b.className).toContain("b-p");
  fireEvent.click(b);
  expect(onNewTask).toHaveBeenCalled();
});

it("a waiting card shows the ask preview and Responder", () => {
  const { onOpen } = setup([task("A-1", { status: { type: "waiting-for-user" } })], { asks: { "A-1": "Which database?" } });
  expect(screen.getByText("Which database?")).not.toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Responder" }));
  expect(onOpen).toHaveBeenCalledWith("A-1");
});

it("Hecha shows the merged PR chip and the date", () => {
  setup([task("A-1", { status: { type: "done" } })], {
    prs: [{ task: "A-1", state: "merged", draft: false, ci: "none", review_decision: null, count: 1, number: 7 }],
    finished: { "A-1": new Date().toISOString() },
  });
  const card = screen.getByRole("link", { name: "A-1 Title A-1" });
  expect(within(card).getByText("#7")).not.toBeNull();
  expect(within(card).getByText("Fusionada")).not.toBeNull();
  expect(within(card).getByText("hace un momento")).not.toBeNull();
});

it("autopilot shows a small icon with its state in the tooltip", () => {
  setup([task("A-1", { autopilot: { state: "paused", reason: "límite", since: "2026-01-01T00:00:00Z" } })]);
  expect(screen.getByRole("img", { name: "Autopiloto en pausa: límite" })).not.toBeNull();
});

it("folds long columns behind «N más»", () => {
  setup(Array.from({ length: 15 }, (_, i) => task(`A-${i + 1}`)));
  expect(screen.getAllByRole("link").length).toBe(12);
  fireEvent.click(screen.getByRole("button", { name: "3 más" }));
  expect(screen.getAllByRole("link").length).toBe(15);
});

it("mounts the first-run checklist with no projects", async () => {
  render(<BoardView tasks={[]} specs={specs} projects={[]} onOpen={() => {}} onNewTask={() => {}} />);
  expect(await screen.findByText("Primeros pasos")).not.toBeNull();
});

it("shows a skeleton while loading", () => {
  setup([], { loading: true });
  expect(document.querySelector("[aria-busy='true']")).not.toBeNull();
  expect(screen.queryByRole("region", { name: "Plan" })).toBeNull();
});
