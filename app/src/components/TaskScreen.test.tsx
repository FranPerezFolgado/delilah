// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/react";
import type { Project, SpecSummary, Task, TaskStatus } from "../api";
import { setLanguage } from "../i18n";
import { TaskScreen } from "./TaskScreen";

vi.mock("../api", async (orig) => {
  const mod = await orig<typeof import("../api")>();
  return {
    ...mod,
    api: new Proxy({}, { get: () => () => new Promise(() => {}) }),
  };
});
vi.mock("./TerminalPane", () => ({ TerminalPane: () => null }));
vi.mock("./FileTree", () => ({ FileTree: () => null }));
vi.mock("./ChangesView", () => ({ ChangesView: () => null }));
vi.mock("./TaskView", () => ({ TaskView: () => null }));
vi.mock("./ProjectView", () => ({ ProjectView: () => null }));
vi.mock("./QuickOpen", () => ({ QuickOpen: () => null }));
vi.mock("./AskCard", () => ({ AskCard: () => null }));

const task = (status: TaskStatus): Task => ({
  key: "PROJ-1",
  title: "A task",
  project: "web",
  workflow: "feature",
  phase: null,
  saving_mode: false,
  handoff: null,
  status,
});
const spec: SpecSummary = { phase: "plan", dir: "specs/001-x", done: 0, total: 3, current: null };
const project: Project = { name: "web", path: "/w/web", kind: "repo", workflow: "feature" };

function head(t?: Task, s?: SpecSummary) {
  const { container } = render(
    <TaskScreen
      owner={t ? t.key : "@web"}
      task={t}
      spec={s}
      project={t ? undefined : project}
      sessions={[]}
      asks={[]}
      askPosition="right"
      onChanged={() => {}}
    />,
  );
  return container.querySelector(".view-head")!;
}

beforeEach(() => setLanguage("es"));
afterEach(() => {
  cleanup();
  localStorage.clear();
});

describe("TaskScreen header phase", () => {
  it("shows the phase pill before the status pill for a running task with a spec", () => {
    const pills = [...head(task({ type: "running" }), spec).querySelectorAll(".pill")];
    expect(pills).toHaveLength(2);
    expect(pills[0].classList.contains("phase-pill")).toBe(true);
    expect(pills[1].classList.contains("phase-pill")).toBe(false);
  });

  it("shows no phase pill for a queued task", () => {
    expect(head(task({ type: "queued" }), spec).querySelector(".phase-pill")).toBeNull();
  });

  it("shows no phase pill for a done task", () => {
    expect(head(task({ type: "done" }), spec).querySelector(".phase-pill")).toBeNull();
  });

  it("shows no phase pill on the project screen", () => {
    expect(head().querySelector(".phase-pill")).toBeNull();
  });
});

describe("TaskScreen saving mode", () => {
  const saving = (status: TaskStatus, on = true) => {
    const { container } = render(
      <TaskScreen
        owner="PROJ-1"
        task={{ ...task(status), saving_mode: on }}
        sessions={[]}
        asks={[]}
        askPosition="right"
        onChanged={() => {}}
      />,
    );
    return container;
  };

  it("shows the marker and the chain with saving mode", () => {
    const c = saving({ type: "running" });
    expect(c.querySelector(".saving-pill")?.textContent).toContain("Modo ahorro");
    expect(c.querySelector(".chain")).not.toBeNull();
    expect(c.textContent).not.toContain("Activar modo ahorro");
  });

  it("shows neither without it, only the activate action while running", () => {
    const c = saving({ type: "running" }, false);
    expect(c.querySelector(".saving-pill")).toBeNull();
    expect(c.querySelector(".chain")).toBeNull();
    expect(c.textContent).toContain("Activar modo ahorro");
  });

  it("offers no activate action on a done task", () => {
    expect(saving({ type: "done" }, false).textContent).not.toContain("Activar modo ahorro");
  });
});
