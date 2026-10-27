// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { api, type SubtaskSummary, type Task } from "../api";
import { setLanguage } from "../i18n";
import { SubtaskList } from "./SubtaskList";

const parent = { key: "P-1", title: "Parent", kind: "parent_task", subtask_progress: { merged: 1, counted: 2, dropped: 1, ready: false } } as Task;
const sub = (over: Partial<SubtaskSummary>): SubtaskSummary => ({ key: "S-1", title: "One", status: { type: "running" }, archived: false, dropped: false, pr: null, ...over });

beforeEach(() => setLanguage("es"));
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

it("lists key, title, state, PR state and dropped subtasks, with the progress", async () => {
  vi.spyOn(api, "subtasks").mockResolvedValue([
    sub({}),
    sub({ key: "S-2", title: "Two", status: { type: "done" }, archived: true, pr: { url: "https://x/2", state: "merged" } }),
    sub({ key: "S-3", title: "Three", archived: true, dropped: true }),
  ]);
  const { findByText, getByText, container } = render(<SubtaskList task={parent} />);
  await findByText("Two");
  expect(getByText("1 de 2 subtareas integradas")).not.toBeNull();
  expect(getByText("PR integrado").getAttribute("href")).toBe("https://x/2");
  expect(container.querySelectorAll(".subtask").length).toBe(3);
  expect(container.querySelector(".subtask.dropped")!.textContent).toContain("descartada");
});

it("opens a subtask and asks for a new one", async () => {
  vi.spyOn(api, "subtasks").mockResolvedValue([sub({})]);
  const onOpenTask = vi.fn();
  const onNewSubtask = vi.fn();
  const { findByText, getByText } = render(<SubtaskList task={parent} onOpenTask={onOpenTask} onNewSubtask={onNewSubtask} />);
  fireEvent.click(await findByText("S-1"));
  expect(onOpenTask).toHaveBeenCalledWith("S-1");
  fireEvent.click(getByText("Nueva subtarea"));
  expect(onNewSubtask).toHaveBeenCalledWith(parent);
});

it("says so when there are no subtasks", async () => {
  vi.spyOn(api, "subtasks").mockResolvedValue([]);
  const { findByText } = render(<SubtaskList task={{ ...parent, subtask_progress: { merged: 0, counted: 0, dropped: 0, ready: false } }} />);
  await findByText("Aún no hay subtareas.");
  await waitFor(() => expect(api.subtasks).toHaveBeenCalledWith("P-1"));
});
