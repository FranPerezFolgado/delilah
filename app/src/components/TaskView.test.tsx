// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/react";
import { api, type Task } from "../api";
import { TaskView } from "./TaskView";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const base: Task = {
  key: "K-1",
  title: "Do it",
  project: "p",
  workflow: "default",
  phase: null,
  status: { type: "running" },
};

const show = (task: Task) => {
  vi.spyOn(api, "events").mockResolvedValue([]);
  return render(
    <TaskView task={task} sessions={[]} onOpenSession={() => {}} onNewSession={async () => {}} onChanged={() => {}} onViewChanges={() => {}} />,
  );
};

describe("TaskView branch", () => {
  it("shows the branch in monospace with its label", () => {
    const { getByText, container } = show({ ...base, branch: "feature/K-1-do-it" });
    const branch = getByText("feature/K-1-do-it");
    expect(branch.className).toContain("mono");
    expect(container.textContent).toContain("Branch");
  });

  it("shows nothing when the task has no branch", () => {
    const { container } = show(base);
    expect(container.textContent).not.toContain("Branch");
    expect(container.querySelector("[data-testid=task-branch]")).toBeNull();
  });
});
