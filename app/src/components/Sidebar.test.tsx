// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/react";
import type { Project, Task, TaskStatus } from "../api";
import { Sidebar } from "./Sidebar";

const web: Project = { name: "web", path: "/w/web", kind: "repo", workflow: "feature" };
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

function setup(tasks: Task[]) {
  const onDelete = vi.fn();
  const view = render(
    <Sidebar
      view="tasks"
      onView={() => {}}
      tasks={tasks}
      projects={[web]}
      sessions={[]}
      selected={null}
      onSelect={() => {}}
      onOpenSession={() => {}}
      onAsk={() => {}}
      onNewTask={() => {}}
      onDelete={onDelete}
      waiting={0}
    />,
  );
  const keys = () => [...view.container.querySelectorAll(".srow .key")].map((k) => k.textContent);
  return { ...view, keys, onDelete };
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
