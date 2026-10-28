// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { api, DEFAULT_PROJECT_SETTINGS, type JiraImportPreview, type JiraImportRow, type Project } from "../api";
import { setLanguage } from "../i18n";
import { JiraImportDialog } from "./JiraImportDialog";

const web: Project = { name: "web", path: "/w/web", kind: "repo", workflow: "feature", settings: DEFAULT_PROJECT_SETTINGS };

const row = (over: Partial<JiraImportRow>): JiraImportRow => ({
  key: "PROJ-11", title: "API", status: "To Do", status_category: "new", assignee: null, mine: false,
  task: null, action: "create", reason: null, skipped: false, selected: true, ...over,
});

const preview = (subtasks: JiraImportRow[]): JiraImportPreview => ({
  linked: true,
  ticket: { key: "PROJ-10", title: "Checkout", status: "In Progress", is_subtask: false },
  parent: { action: "create" },
  subtasks,
  gone: [],
});

const rows = [
  row({}),
  row({ key: "PROJ-12", title: "UI", assignee: "Ana", mine: false, selected: false }),
  row({ key: "PROJ-13", title: "Docs", status: "Done", status_category: "done", selected: false }),
  row({ key: "PROJ-14", title: "Old", action: "linked", task: { key: "PROJ-14", kind: "task", status: "queued", parent_task: null, archived: false }, selected: false }),
];

const open = (p: JiraImportPreview = preview(rows)) => {
  vi.spyOn(api, "jiraImportPreview").mockResolvedValue(p);
  vi.spyOn(api, "projects").mockResolvedValue([web]);
  const props = { onClose: vi.fn(), onDone: vi.fn() };
  render(<JiraImportDialog issueKey="PROJ-10" {...props} />);
  return props;
};
const box = (key: string) => screen.getByText(key).closest("li")!.querySelector("input") as HTMLInputElement;

beforeEach(() => setLanguage("es"));
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("JiraImportDialog create mode", () => {
  it("lists rows with status, assignee and tags, ticked by default as `selected`", async () => {
    open();
    await screen.findByText("API");
    expect(box("PROJ-11").checked).toBe(true);
    expect(box("PROJ-12").checked).toBe(false);
    expect(screen.getByText("asignada a Ana")).toBeTruthy();
    expect(screen.getByText("hecha en Jira")).toBeTruthy();
    expect(screen.getByText("ya es una tarea")).toBeTruthy();
    expect(box("PROJ-14").disabled).toBe(true);
  });

  it("needs a project, and the toggles change `add`", async () => {
    const props = open();
    const imp = vi.spyOn(api, "jiraImport").mockResolvedValue({
      parent_task: "PROJ-10", created: ["PROJ-11", "PROJ-12"], attached: [],
      left_out: [{ key: "PROJ-13", reason: "started" }], skipped: [], unskipped: [],
    });
    await screen.findByText("API");
    const create = screen.getByRole("button", { name: "Crear" }) as HTMLButtonElement;
    expect(create.disabled).toBe(true);
    fireEvent.change(screen.getByLabelText("Proyecto"), { target: { value: "web" } });
    expect(create.disabled).toBe(false);
    fireEvent.click(box("PROJ-12"));
    fireEvent.click(create);
    await waitFor(() => expect(imp).toHaveBeenCalledWith("PROJ-10", { project: "web", add: ["PROJ-11", "PROJ-12"], skip: [], unskip: [] }));
    await screen.findByText("Tarea padre PROJ-10 creada.");
    expect(screen.getByText("Creadas: PROJ-11, PROJ-12")).toBeTruthy();
    expect(screen.getByText("PROJ-13").parentElement?.textContent).toContain("started");
    expect(props.onDone).toHaveBeenCalled();
  });

  it("shows unreachable with a retry that reloads the preview", async () => {
    open({ linked: false, reason: "unreachable" });
    await screen.findByText("No se pudo hablar con Jira.");
    vi.mocked(api.jiraImportPreview).mockResolvedValue(preview(rows));
    fireEvent.click(screen.getByRole("button", { name: "Reintentar" }));
    await screen.findByText("API");
  });

  it("keeps the dialog open with the error when the import fails", async () => {
    open();
    vi.spyOn(api, "jiraImport").mockRejectedValue(new Error("boom"));
    await screen.findByText("API");
    fireEvent.change(screen.getByLabelText("Proyecto"), { target: { value: "web" } });
    fireEvent.click(screen.getByRole("button", { name: "Crear" }));
    await screen.findByText("boom");
    expect((screen.getByRole("button", { name: "Crear" }) as HTMLButtonElement).disabled).toBe(false);
  });
});
