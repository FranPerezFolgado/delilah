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

it("shows a hint with the number of columns cut off at the right edge and scrolls on click", () => {
  const defs = [
    Object.getOwnPropertyDescriptor(HTMLElement.prototype, "clientWidth"),
    Object.getOwnPropertyDescriptor(HTMLElement.prototype, "offsetLeft"),
    Object.getOwnPropertyDescriptor(HTMLElement.prototype, "offsetWidth"),
  ];
  const isBoard = (el: HTMLElement) => el.classList.contains("board");
  const isCol = (el: HTMLElement) => el.classList.contains("col");
  const index = (el: HTMLElement) => [...el.parentElement!.children].indexOf(el);
  Object.defineProperty(HTMLElement.prototype, "clientWidth", { configurable: true, get() { return isBoard(this) ? 700 : 0; } });
  Object.defineProperty(HTMLElement.prototype, "offsetWidth", { configurable: true, get() { return isCol(this) ? 220 : 0; } });
  Object.defineProperty(HTMLElement.prototype, "offsetLeft", { configurable: true, get() { return isCol(this) ? index(this) * 232 : 0; } });
  const scrollBy = vi.fn();
  HTMLElement.prototype.scrollBy = scrollBy;
  try {
    setup([task("A-1")]);
    // 6 columns at 232 px: the 4th ends at 916 > 701, so columns 4, 5 and 6 are cut.
    const hint = screen.getByRole("button", { name: /columnas más/ });
    expect(hint.textContent).toBe("3 columnas más ›");
    fireEvent.click(hint);
    expect(scrollBy).toHaveBeenCalledWith(expect.objectContaining({ left: 700 }));
  } finally {
    ["clientWidth", "offsetLeft", "offsetWidth"].forEach((k, i) => {
      if (defs[i]) Object.defineProperty(HTMLElement.prototype, k, defs[i]!);
      else delete (HTMLElement.prototype as unknown as Record<string, unknown>)[k];
    });
  }
});

it("has no hint when every column fits", () => {
  setup([task("A-1")]);
  expect(screen.queryByRole("button", { name: /columnas más/ })).toBeNull();
});

it("each card shows its own task's PR, never another task's", () => {
  setup([task("A-1", { status: { type: "done" } }), task("A-2", { status: { type: "running" } })], {
    prs: [
      { task: "A-2", state: "open", draft: false, ci: "failure", review_decision: null, count: 1, number: 14 },
      { task: "A-1", state: "merged", draft: false, ci: "success", review_decision: null, count: 1, number: 12 },
    ],
  });
  const done = screen.getByRole("link", { name: "A-1 Title A-1" });
  expect(within(done).getByText("#12")).not.toBeNull();
  expect(within(done).queryByText("#14")).toBeNull();
  expect(within(screen.getByRole("link", { name: "A-2 Title A-2" })).getByText("#14")).not.toBeNull();
});

it("one hidden column reads in the singular", () => {
  const defs = ["clientWidth", "offsetLeft", "offsetWidth"].map((k) => Object.getOwnPropertyDescriptor(HTMLElement.prototype, k));
  const isBoard = (el: HTMLElement) => el.classList.contains("board");
  const isCol = (el: HTMLElement) => el.classList.contains("col");
  const index = (el: HTMLElement) => [...el.parentElement!.children].indexOf(el);
  Object.defineProperty(HTMLElement.prototype, "clientWidth", { configurable: true, get() { return isBoard(this) ? 1160 : 0; } });
  Object.defineProperty(HTMLElement.prototype, "offsetWidth", { configurable: true, get() { return isCol(this) ? 220 : 0; } });
  Object.defineProperty(HTMLElement.prototype, "offsetLeft", { configurable: true, get() { return isCol(this) ? index(this) * 232 : 0; } });
  try {
    setup([task("A-1")]);
    // only the 6th column (ends at 1380 > 1161) is cut
    expect(screen.getByRole("button", { name: /columna más/ }).textContent).toBe("1 columna más ›");
  } finally {
    ["clientWidth", "offsetLeft", "offsetWidth"].forEach((k, i) => {
      if (defs[i]) Object.defineProperty(HTMLElement.prototype, k, defs[i]!);
      else delete (HTMLElement.prototype as unknown as Record<string, unknown>)[k];
    });
  }
});
