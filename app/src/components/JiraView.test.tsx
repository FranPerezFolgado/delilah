// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { api, ApiError, type JiraIssue, type JiraSettings } from "../api";
import { setLanguage } from "../i18n";
import { JiraView } from "./JiraView";

const settings: JiraSettings = { mode: "sample", site: null, email: null, jql: "" };

const issue = (over: Partial<JiraIssue>): JiraIssue => ({
  key: "PROJ-1", summary: "Reintentos con backoff", issue_type: "Story", priority: "Media", status: "En curso",
  status_category: "indeterminate", assignee: "Ana Gil", reporter: null, epic: { key: "PROJ-9", summary: "Fiabilidad" },
  subtask: false, parent: null, sprint: { name: "Sprint 4", state: "active" }, components: [], description: "Texto",
  url: "https://jira.example/browse/PROJ-1", ...over,
});

const rows = [
  issue({ subtask_count: 20, subtasks_done: 8 }),
  issue({ key: "PROJ-2", summary: "Otra cosa", status: "Por hacer", status_category: "new", priority: "Alta", epic: null, sprint: null }),
];

const view = (onAnalyze = vi.fn(), onOpenTask = vi.fn()) => {
  render(<JiraView onAnalyze={onAnalyze} onOpenTask={onOpenTask} />);
  return { onAnalyze, onOpenTask };
};

beforeEach(() => {
  setLanguage("es");
  vi.spyOn(api, "jira").mockResolvedValue({ settings });
  vi.spyOn(api, "jiraIssues").mockResolvedValue(rows);
  vi.spyOn(api, "jiraSubtasks").mockResolvedValue([]);
  vi.spyOn(api, "tasks").mockResolvedValue([]);
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("JiraView list", () => {
  it("shows two-line rows, the priority only for Alta/Crítica, and no avatars for «Asignados a mí»", async () => {
    view();
    await screen.findByText("Otra cosa");
    expect(screen.getByText("Fiabilidad · Sprint 4 · 8/20 subtareas")).toBeTruthy();
    const prios = [...document.querySelectorAll(".jv-prio")].map((e) => e.textContent);
    expect(prios).toEqual(["Alta"]);
    expect(document.querySelector(".javatar")).toBeNull();
  });

  it("filters through labelled dropdown chips and shows avatars once «De quién» changes", async () => {
    const spy = vi.mocked(api.jiraIssues);
    view();
    await screen.findByText("Otra cosa");
    fireEvent.click(screen.getByText("De quién").closest("button")!);
    fireEvent.click(screen.getByRole("menuitem", { name: "Reportados por mí" }));
    await waitFor(() => expect(spy).toHaveBeenLastCalledWith("reported", "open", "all"));
    await waitFor(() => expect(document.querySelector(".javatar")).not.toBeNull());
  });

  it("groups by Épica from the radio", async () => {
    view();
    await screen.findByText("Otra cosa");
    fireEvent.click(screen.getByRole("radio", { name: "Épica" }));
    expect(screen.getByText("Sin épica")).toBeTruthy();
    expect(document.querySelector(".jgroup-name")?.textContent).toBe("Fiabilidad");
  });

  it("shows the empty state and «Quitar filtros» resets", async () => {
    view();
    await screen.findByText("Otra cosa");
    fireEvent.change(screen.getByLabelText("Filtrar tickets"), { target: { value: "zzz" } });
    expect(screen.getByText("Ningún ticket con estos filtros")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Quitar filtros" }));
    expect(screen.getByText("Otra cosa")).toBeTruthy();
  });

  it("on a 401 shows the banner over the last data, and «Reconectar» opens the form", async () => {
    view();
    await screen.findByText("Otra cosa");
    vi.mocked(api.jiraIssues).mockRejectedValue(new ApiError("unauthorized", 401));
    fireEvent.click(screen.getByLabelText("Recargar"));
    await screen.findByText("Jira rechazó el token (401)");
    expect(screen.getByText("Otra cosa")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Reconectar" }));
    expect(screen.getByText("Conectar Jira")).toBeTruthy();
  });
});

describe("JiraView detail", () => {
  it("offers «Analizar con Claude» when there is no task", async () => {
    const { onAnalyze } = view();
    await screen.findByText("Otra cosa");
    fireEvent.click(screen.getByRole("button", { name: "Analizar con Claude" }));
    expect(onAnalyze).toHaveBeenCalledWith(rows[0]);
    expect(screen.getByText("Abrir en Jira ↗")).toBeTruthy();
  });

  it("shows the linked task as one clickable row", async () => {
    vi.mocked(api.jiraIssues).mockResolvedValue([{ ...rows[0], task_status: "running" }]);
    const { onOpenTask } = view();
    fireEvent.click(await screen.findByText("Abrir ›"));
    expect(onOpenTask).toHaveBeenCalledWith("PROJ-1");
    expect(screen.queryByRole("button", { name: "Analizar con Claude" })).toBeNull();
  });

  it("lists the subtasks with progress; only «En curso» is open", async () => {
    const sub = (key: string, status: string, status_category: string) =>
      issue({ key, summary: `sub ${key}`, status, status_category, subtask: true, parent: { key: "PROJ-1", summary: null } });
    vi.mocked(api.jiraSubtasks).mockResolvedValue([
      sub("PROJ-11", "En curso", "indeterminate"), sub("PROJ-12", "Hecho", "done"), sub("PROJ-13", "Hecho", "done"),
    ]);
    view();
    await screen.findByText("sub PROJ-11");
    expect(screen.getByText("2/3 hechas")).toBeTruthy();
    expect(screen.queryByText("sub PROJ-12")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: /Hecho/ }));
    expect(screen.getByText("sub PROJ-12")).toBeTruthy();
  });
});

describe("JiraView connect form", () => {
  it("offers Cloud / Server / Datos de ejemplo and shows inline field errors", async () => {
    vi.mocked(api.jira).mockResolvedValue({ settings: null });
    const connect = vi.spyOn(api, "connectJira");
    view();
    await screen.findByRole("radio", { name: "Cloud" });
    expect(screen.getByRole("radio", { name: "Server" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Conectar" }));
    expect(screen.getByText("Falta la URL del sitio")).toBeTruthy();
    expect(screen.getByText("Falta el email")).toBeTruthy();
    expect(screen.getByText("Falta el token")).toBeTruthy();
    expect(connect).not.toHaveBeenCalled();
  });

  it("connects with sample data", async () => {
    vi.mocked(api.jira).mockResolvedValue({ settings: null });
    const connect = vi.spyOn(api, "connectJira").mockResolvedValue({ account: "x", settings });
    view();
    fireEvent.click(await screen.findByRole("radio", { name: "Datos de ejemplo" }));
    fireEvent.click(screen.getByRole("button", { name: "Conectar" }));
    await waitFor(() => expect(connect).toHaveBeenCalledWith({ mode: "sample", jql: undefined }));
  });
});
