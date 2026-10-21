// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/react";
import type { Project, Session, SpecSummary, Task, TaskStatus } from "../api";
import { sessionFor, sessionTab } from "../editor/session";
import { setLanguage } from "../i18n";
import { TaskScreen } from "./TaskScreen";

vi.mock("../api", async (orig) => {
  const mod = await orig<typeof import("../api")>();
  return {
    ...mod,
    api: new Proxy({}, { get: () => () => new Promise(() => {}) }),
  };
});
vi.mock("./TerminalPane", () => ({ TerminalPane: () => <div data-testid="terminal" /> }));
vi.mock("./ConversationView", () => ({ ConversationView: () => <div data-testid="conversation" /> }));
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

describe("TaskScreen conversation sessions", () => {
  const conv: Session = {
    id: "s1",
    task: "@web",
    kind: "claude",
    label: "Idea",
    primary: false,
    closed: false,
    live: true,
    read_only: false,
    state: "waiting",
    conversation: true,
  };
  const open = (s: Session) => {
    sessionFor("@web").active = sessionTab(s.id);
    return render(
      <TaskScreen owner="@web" project={project} sessions={[s]} asks={[]} askPosition="right" onChanged={() => {}} onConvert={() => {}} />,
    );
  };

  it("shows the conversation view, not the terminal, with no convert bar", () => {
    const { queryByTestId, container } = open(conv);
    expect(queryByTestId("conversation")).not.toBeNull();
    expect(queryByTestId("terminal")).toBeNull();
    expect(container.querySelector(".tterm .ebar")).toBeNull();
  });

  it("toggles to the terminal and back", () => {
    const { queryByTestId, getByText } = open(conv);
    fireEvent.click(getByText("Ver terminal"));
    expect(queryByTestId("terminal")).not.toBeNull();
    expect(queryByTestId("conversation")).toBeNull();
    fireEvent.click(getByText("Ver conversación"));
    expect(queryByTestId("conversation")).not.toBeNull();
    expect(queryByTestId("terminal")).toBeNull();
  });

  it("keeps the terminal for a plain session", () => {
    const { queryByTestId } = open({ ...conv, conversation: false });
    expect(queryByTestId("terminal")).not.toBeNull();
    expect(queryByTestId("conversation")).toBeNull();
  });
});
