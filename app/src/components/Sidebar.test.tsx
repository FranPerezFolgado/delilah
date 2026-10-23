// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/react";
import { DEFAULT_PROJECT_SETTINGS, type Project, type Session, type Task, type TaskStatus } from "../api";
import { Sidebar } from "./Sidebar";
import { setLanguage } from "../i18n";

const web: Project = {
  name: "web",
  path: "/w/web",
  kind: "repo",
  workflow: "feature",
  settings: DEFAULT_PROJECT_SETTINGS,
};
const task = (key: string, status: TaskStatus): Task => ({
  key,
  title: key,
  project: "web",
  workflow: "feature",
  phase: null,
  saving_mode: false,
  handoff: null,
  status,
});

function setup(tasks: Task[], sessions: Session[] = []) {
  const onDelete = vi.fn();
  const onIdea = vi.fn();
  const onOpenIdeas = vi.fn();
  const view = render(
    <Sidebar
      view="tasks"
      onView={() => {}}
      tasks={tasks}
      projects={[web]}
      sessions={sessions}
      selected={null}
      onSelect={() => {}}
      onOpenSession={() => {}}
      onAsk={() => {}}
      onIdea={onIdea}
      onOpenIdeas={onOpenIdeas}
      onNewTask={() => {}}
      onDelete={onDelete}
      waiting={0}
    />,
  );
  const keys = () => [...view.container.querySelectorAll(".srow .key")].map((k) => k.textContent);
  return { ...view, keys, onDelete, onIdea, onOpenIdeas };
}

afterEach(() => {
  cleanup();
  localStorage.clear();
});

describe("Sidebar backlog", () => {
  it("keeps backlog tasks folded apart, oldest first, after the work in progress", () => {
    const { container, keys } = setup([
      task("PROJ-1", { type: "queued" }),
      task("PROJ-2", { type: "running" }),
      task("PROJ-3", { type: "queued" }),
      task("PROJ-4", { type: "waiting-for-user" }),
    ]);
    // Urgent first; the backlog is not pending work.
    expect(keys()).toEqual(["PROJ-4", "PROJ-2"]);
    const head = container.querySelector<HTMLButtonElement>(".bhead")!;
    expect(head.textContent).toContain("(2)");
    expect(head.getAttribute("aria-expanded")).toBe("false");

    fireEvent.click(head);
    expect(keys()).toEqual(["PROJ-4", "PROJ-2", "PROJ-1", "PROJ-3"]);
  });

  it("shows no backlog section without backlog tasks", () => {
    const { container } = setup([task("PROJ-2", { type: "running" })]);
    expect(container.querySelector(".bhead")).toBeNull();
  });

  it("offers deleting a task from its row's context menu", () => {
    const tasks = [task("PROJ-2", { type: "running" })];
    const { container, onDelete } = setup(tasks);
    fireEvent.contextMenu(container.querySelector(".srow")!);
    fireEvent.click(container.querySelector(".menu.ctx button")!);
    expect(onDelete).toHaveBeenCalledWith(tasks[0]);
  });
});

describe("Sidebar ideas", () => {
  beforeEach(() => setLanguage("es"));
  afterEach(() => setLanguage("en"));
  it("starts an idea conversation from the project's second button", () => {
    const { getByLabelText, onIdea } = setup([]);
    fireEvent.click(getByLabelText("Nueva tarea con Claude en web"));
    expect(onIdea).toHaveBeenCalledWith("web");
  });

  const idea = (id: string, state = "waiting") => ({ id, task: "@web", kind: "claude", label: "Idea " + id, primary: false, closed: false, live: true, read_only: false, state, conversation: true }) as unknown as Session;
  const ro = { id: "r1", task: "@web", kind: "claude", label: "Ask", primary: false, closed: false, live: true, read_only: true, state: "idle", conversation: false } as unknown as Session;

  it("shows no Ideas row without idea conversations", () => {
    const { container } = setup([], [ro]);
    expect(container.querySelector(".srow.ideas")).toBeNull();
  });

  it("shows one Ideas row with the count for a single idea", () => {
    const { container } = setup([], [idea("a")]);
    const row = container.querySelector(".srow.ideas")!;
    expect(row.textContent).toContain("Ideas");
    expect(row.querySelector(".pcount")!.textContent).toBe("1");
  });

  it("collapses many ideas into one first row, leaving other sessions alone", () => {
    const { container } = setup([], [ro, idea("a"), idea("b"), idea("c")]);
    const rows = [...container.querySelectorAll(".srow")];
    expect(rows[0].classList.contains("ideas")).toBe(true);
    expect(rows[0].querySelector(".pcount")!.textContent).toBe("3");
    expect(rows.length).toBe(2);
    expect(rows[1].textContent).toContain("Ask");
  });

  it("opens the project's ideas, only the ideas and in list order", () => {
    const { getByRole, onOpenIdeas } = setup([], [idea("a"), ro, idea("b"), idea("c")]);
    fireEvent.click(getByRole("button", { name: "Ideas de web (3)" }));
    expect(onOpenIdeas).toHaveBeenCalledTimes(1);
    expect(onOpenIdeas).toHaveBeenCalledWith("@web", [idea("a"), idea("b"), idea("c")]);
  });

  it("highlights the count when any idea waits", () => {
    const { container } = setup([], [idea("a", "working"), idea("b", "waiting")]);
    expect(container.querySelector(".srow.ideas .pcount")!.classList.contains("c-wait")).toBe(true);
  });

  it("does not highlight the count when no idea waits", () => {
    const { container } = setup([], [idea("a", "working"), idea("b", "working")]);
    expect(container.querySelector(".srow.ideas .pcount")!.classList.contains("c-wait")).toBe(false);
  });

  it("shows the idea icon on the row", () => {
    const { container } = setup([], [idea("a")]);
    expect(container.querySelector(".srow.ideas svg.idea")).not.toBeNull();
  });

  it("orders the Ideas row, then other sessions, then tasks", () => {
    const { container } = setup([task("PROJ-1", { type: "running" })], [ro, idea("a")]);
    const rows = [...container.querySelectorAll(".srow")];
    expect(rows.length).toBe(3);
    expect(rows[0].classList.contains("ideas")).toBe(true);
    expect(rows[1].textContent).toContain("Ask");
    expect(rows[2].querySelector(".key")!.textContent).toBe("PROJ-1");
  });

  it("hides the row with the folded project, whose count still includes ideas", () => {
    const { container, getByLabelText } = setup([task("PROJ-1", { type: "running" })], [ro, idea("a"), idea("b")]);
    fireEvent.click(getByLabelText("Plegar"));
    expect(container.querySelector(".srow.ideas")).toBeNull();
    expect(container.querySelector(".pcount")!.textContent).toBe("4");
  });
});
