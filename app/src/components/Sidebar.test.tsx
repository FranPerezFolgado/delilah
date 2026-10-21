// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/react";
import type { Project, Session, Task, TaskStatus } from "../api";
import { Sidebar } from "./Sidebar";
import { setLanguage } from "../i18n";

const web: Project = { name: "web", path: "/w/web", kind: "repo", workflow: "feature" };
const task = (key: string, status: TaskStatus): Task => ({
  key,
  title: key,
  project: "web",
  workflow: "feature",
  phase: null,
  status,
});

function setup(tasks: Task[], sessions: Session[] = []) {
  const onDelete = vi.fn();
  const onIdea = vi.fn();
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
      onNewTask={() => {}}
      onDelete={onDelete}
      waiting={0}
    />,
  );
  const keys = () => [...view.container.querySelectorAll(".srow .key")].map((k) => k.textContent);
  return { ...view, keys, onDelete, onIdea };
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

  it("lists a conversation session with the idea icon", () => {
    const s = { id: "s1", task: "@web", kind: "claude", label: "Idea", primary: false, closed: false, live: true, read_only: false, state: "waiting", conversation: true } as Session;
    const { container } = setup([], [s]);
    expect(container.querySelector(".srow svg.idea")).not.toBeNull();
  });
});
