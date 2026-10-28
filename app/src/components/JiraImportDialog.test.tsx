// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { api, DEFAULT_PROJECT_SETTINGS, type JiraImportParent, type JiraImportPreview, type JiraImportRow, type JiraImportTask, type Project } from "../api";
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

const withParent = (parent: JiraImportParent): JiraImportPreview => ({ ...(preview(rows) as Extract<JiraImportPreview, { linked: true }>), parent });

const rows = [
  row({}),
  row({ key: "PROJ-12", title: "UI", assignee: "Ana", mine: false, selected: false }),
  row({ key: "PROJ-13", title: "Docs", status: "Done", status_category: "done", selected: false }),
  row({ key: "PROJ-14", title: "Old", action: "linked", task: { key: "PROJ-14", kind: "normal", status: { type: "queued" }, parent_task: null, archived: false }, selected: false }),
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

  it("renders convert and existing parent lines without asking for a project", async () => {
    open(withParent({ action: "convert" }));
    await screen.findByText("API");
    expect(screen.getByText("PROJ-10 pasa a ser la tarea padre.")).toBeTruthy();
    expect(screen.queryByLabelText("Proyecto")).toBeNull();
    expect((screen.getByRole("button", { name: "Crear" }) as HTMLButtonElement).disabled).toBe(false);
    cleanup();
    open(withParent({ action: "existing" }));
    await screen.findByText("API");
    expect(screen.getByText("Se añade a la tarea padre PROJ-10.")).toBeTruthy();
  });

  it("shows the refusal and disables confirm", async () => {
    open(withParent({ action: "refused", code: "x", message: "No se puede" }));
    await screen.findByText("No se puede");
    expect((screen.getByRole("button", { name: "Crear" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("marks attach rows and leaves conflict rows unticked and disabled with the reason", async () => {
    const task: JiraImportTask = { key: "PROJ-15", kind: "normal", status: { type: "queued" }, parent_task: null, archived: false };
    open(preview([
      row({ key: "PROJ-15", title: "Att", action: "attach", task }),
      row({ key: "PROJ-16", title: "Bad", action: "conflict", task, reason: "ya tiene otro padre" }),
    ]));
    await screen.findByText("Att");
    expect(screen.getByText("ya es una tarea, se unirá")).toBeTruthy();
    expect(box("PROJ-15").checked).toBe(true);
    expect(box("PROJ-16").checked).toBe(false);
    expect(box("PROJ-16").disabled).toBe(true);
    expect(screen.getByText("ya tiene otro padre")).toBeTruthy();
  });
});

describe("JiraImportDialog add mode", () => {
  const task: JiraImportTask = { key: "PROJ-15", kind: "normal", status: { type: "queued" }, parent_task: null, archived: false };
  const addRows = [
    row({}),
    row({ key: "PROJ-15", title: "Att", action: "attach", task }),
    row({ key: "PROJ-14", title: "Old", action: "linked", task: { ...task, key: "PROJ-14", parent_task: "PROJ-10" }, selected: false }),
    row({ key: "PROJ-16", title: "Bad", action: "conflict", task, reason: "otro padre", selected: false }),
    row({ key: "PROJ-17", title: "Left", skipped: true, selected: false }),
  ];
  const openAdd = () => {
    vi.spyOn(api, "jiraImportPreview").mockResolvedValue({ ...(preview(addRows) as Extract<JiraImportPreview, { linked: true }>), parent: { action: "existing" } });
    vi.spyOn(api, "projects").mockResolvedValue([web]);
    const props = { onClose: vi.fn(), onDone: vi.fn() };
    render(<JiraImportDialog issueKey="PROJ-10" mode="add" {...props} />);
    return props;
  };

  it("lists only create/attach rows plus the skipped ones, with no project picker", async () => {
    openAdd();
    await screen.findByText("API");
    expect(screen.getByText("Att")).toBeTruthy();
    expect(screen.queryByText("Old")).toBeNull();
    expect(screen.queryByText("Bad")).toBeNull();
    expect(screen.queryByLabelText("Proyecto")).toBeNull();
    expect(screen.getByText("Omitidas (1)")).toBeTruthy();
    expect(screen.getByText("Left")).toBeTruthy();
  });

  it("sends a per-row Skip in `skip` and an Undo in `unskip`", async () => {
    openAdd();
    const imp = vi.spyOn(api, "jiraImport").mockResolvedValue({ parent_task: "PROJ-10", created: [], attached: [], left_out: [], skipped: [], unskipped: [] });
    await screen.findByText("API");
    fireEvent.click(screen.getByRole("button", { name: "Omitir PROJ-15" }));
    expect(screen.getByText("Omitidas (2)")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Deshacer PROJ-17" }));
    expect(screen.getByText("Omitidas (1)")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Añadir" }));
    await waitFor(() => expect(imp).toHaveBeenCalledWith("PROJ-10", { add: ["PROJ-11"], skip: ["PROJ-15"], unskip: ["PROJ-17"] }));
  });
});
