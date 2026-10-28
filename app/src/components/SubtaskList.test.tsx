// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { api, type JiraImportPreview, type JiraImportRow, type SubtaskSummary, type Task } from "../api";
import { setLanguage } from "../i18n";
import { SubtaskList } from "./SubtaskList";

const parent = { key: "P-1", title: "Parent", kind: "parent_task", subtask_progress: { merged: 1, counted: 2, dropped: 1, ready: false } } as Task;
const sub = (over: Partial<SubtaskSummary>): SubtaskSummary => ({ key: "S-1", title: "One", status: { type: "running" }, archived: false, dropped: false, pr: null, ...over });

const jrow = (over: Partial<JiraImportRow>): JiraImportRow => ({
  key: "P-11", title: "New", status: "To Do", status_category: "new", assignee: null, mine: false,
  task: null, action: "create", reason: null, skipped: false, selected: true, ...over,
});
const prev = (subtasks: JiraImportRow[]): JiraImportPreview => ({
  linked: true, ticket: { key: "P-1", title: "Parent", status: "x", is_subtask: false }, parent: { action: "existing" }, subtasks, gone: [],
});

beforeEach(() => {
  setLanguage("es");
  vi.spyOn(api, "subtasks").mockResolvedValue([sub({})]);
  vi.spyOn(api, "jiraImportPreview").mockRejectedValue(new Error("no"));
});
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

it("shows the banner with the count of new create/attach rows and opens the dialog in add mode", async () => {
  vi.spyOn(api, "jiraImportPreview").mockResolvedValue(prev([
    jrow({}), jrow({ key: "P-12", action: "attach" }), jrow({ key: "P-13", skipped: true }), jrow({ key: "P-14", action: "linked" }),
  ]));
  vi.spyOn(api, "projects").mockResolvedValue([]);
  const { findByText, getByRole } = render(<SubtaskList task={parent} />);
  await findByText("2 subtareas nuevas de Jira");
  fireEvent.click(getByRole("button", { name: "Añadir" }));
  await findByText("Añadir subtareas de P-1");
});

it("hides the banner at 0 and lists the subtasks before the preview resolves", async () => {
  vi.spyOn(api, "jiraImportPreview").mockReturnValue(new Promise(() => {}));
  const { findByText, queryByText } = render(<SubtaskList task={parent} />);
  await findByText("One");
  expect(queryByText(/subtareas? nuevas? de Jira/)).toBeNull();
  cleanup();
  vi.spyOn(api, "jiraImportPreview").mockResolvedValue(prev([jrow({ action: "linked" })]));
  const r = render(<SubtaskList task={parent} />);
  await r.findByText("One");
  await waitFor(() => expect(api.jiraImportPreview).toHaveBeenCalled());
  expect(r.queryByText(/de Jira/)).toBeNull();
});

it("shows a quiet note when Jira is not connected and nothing for not-found/unreachable", async () => {
  vi.spyOn(api, "jiraImportPreview").mockResolvedValue({ linked: false, reason: "not-connected" });
  const a = render(<SubtaskList task={parent} />);
  await a.findByText("Jira no está conectado.");
  cleanup();
  for (const reason of ["not-found", "unreachable"] as const) {
    vi.mocked(api.jiraImportPreview).mockResolvedValue({ linked: false, reason });
    const b = render(<SubtaskList task={parent} />);
    await b.findByText("One");
    await waitFor(() => expect(api.jiraImportPreview).toHaveBeenCalled());
    expect(b.queryByText("Jira no está conectado.")).toBeNull();
    expect(b.container.querySelector(".subtasks-note")).toBeNull();
    cleanup();
  }
});

it("lists the skipped rows with an Undo that unskips them", async () => {
  vi.spyOn(api, "jiraImportPreview").mockResolvedValue(prev([jrow({ key: "P-13", title: "Left out", skipped: true })]));
  const imp = vi.spyOn(api, "jiraImport").mockResolvedValue({ parent_task: "P-1", created: [], attached: [], left_out: [], skipped: [], unskipped: ["P-13"] });
  const { findByText, getByRole } = render(<SubtaskList task={parent} />);
  await findByText("Omitidas (1)");
  await findByText("Left out");
  fireEvent.click(getByRole("button", { name: "Deshacer P-13" }));
  await waitFor(() => expect(imp).toHaveBeenCalledWith("P-1", { add: [], skip: [], unskip: ["P-13"] }));
});
