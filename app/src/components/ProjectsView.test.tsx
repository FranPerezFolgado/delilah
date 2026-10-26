// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { setLanguage } from "../i18n";
import { DEFAULT_PROJECT_SETTINGS, api, type Project, type Task } from "../api";
import { ProjectsView } from "./ProjectsView";

beforeEach(() => setLanguage("es"));
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const app: Project = { name: "app", path: "/dev/app", kind: "repo", workflow: "feature", settings: { ...DEFAULT_PROJECT_SETTINGS, workspace: "worktree" } };
const multi: Project = { name: "multi", path: "/dev/multi", kind: "parent", workflow: "feature", settings: DEFAULT_PROJECT_SETTINGS };
const task = (over: Partial<Task> & { key: string }): Task => ({
  title: "T", project: "app", workflow: "feature", phase: null, status: { type: "running" }, ...over,
} as Task);

describe("ProjectsView", () => {
  it("shows the empty state and opens the add dialog", () => {
    const { getAllByRole, getByRole, queryByRole, container } = render(<ProjectsView projects={[]} onChanged={() => {}} />);
    expect(container.textContent).toContain("Todavía no hay proyectos.");
    expect(queryByRole("dialog")).toBeNull();
    fireEvent.click(getAllByRole("button", { name: "Añadir proyecto" })[0]);
    expect(getByRole("dialog").textContent).toContain("Añadir proyecto");
  });

  it("has no Tipo column and shows kind, active count, where it works and folder", () => {
    const { container, queryByText, getAllByRole } = render(
      <ProjectsView projects={[app, multi]} tasks={[task({ key: "T-1" }), task({ key: "T-2", status: { type: "done" } as Task["status"] })]} onChanged={() => {}} />,
    );
    expect(queryByText("Tipo")).toBeNull();
    const rows = container.querySelectorAll("tbody tr");
    expect(rows[0].textContent).toContain("Repo · 1 tarea activa");
    expect(rows[0].textContent).toContain("Aislado (worktree)");
    expect(rows[0].textContent).toContain("/dev/app");
    expect(rows[1].textContent).toContain("Multi-repo · 0 tareas activas");
    expect(getAllByRole("button", { name: "Ajustes" }).length).toBe(2);
  });

  it("opens the project from the row", () => {
    const onOpen = vi.fn();
    const { getByText } = render(<ProjectsView projects={[app]} onChanged={() => {}} onOpenProject={onOpen} />);
    fireEvent.click(getByText("app"));
    expect(onOpen).toHaveBeenCalledWith("app");
  });

  it("renames through the ⋯ menu", async () => {
    const update = vi.spyOn(api, "updateProject").mockResolvedValue(app);
    const onChanged = vi.fn();
    const { getByRole, getByLabelText } = render(<ProjectsView projects={[app]} onChanged={onChanged} />);
    fireEvent.click(getByRole("button", { name: /Más acciones de app/ }));
    fireEvent.click(getByRole("menuitem", { name: "Renombrar" }));
    fireEvent.change(getByLabelText("Nombre"), { target: { value: "nuevo" } });
    fireEvent.click(getByRole("button", { name: "Guardar" }));
    await waitFor(() => expect(update).toHaveBeenCalledWith("app", { name: "nuevo" }));
    await waitFor(() => expect(onChanged).toHaveBeenCalled());
  });

  it("removes after confirming", async () => {
    const del = vi.spyOn(api, "deleteProject").mockResolvedValue(null);
    const onChanged = vi.fn();
    const { getByRole } = render(<ProjectsView projects={[app]} onChanged={onChanged} />);
    fireEvent.click(getByRole("button", { name: /Más acciones de app/ }));
    fireEvent.click(getByRole("menuitem", { name: "Quitar…" }));
    fireEvent.click(getByRole("button", { name: "Quitar" }));
    await waitFor(() => expect(del).toHaveBeenCalledWith("app"));
    await waitFor(() => expect(onChanged).toHaveBeenCalled());
  });

  it("blocks removing while a task has a workspace", () => {
    const del = vi.spyOn(api, "deleteProject").mockResolvedValue(null);
    const { getByRole } = render(<ProjectsView projects={[app]} tasks={[task({ key: "T-1", branch: "feat/x" })]} onChanged={() => {}} />);
    fireEvent.click(getByRole("button", { name: /Más acciones de app/ }));
    fireEvent.click(getByRole("menuitem", { name: "Quitar…" }));
    const action = getByRole("button", { name: "Quitar" }) as HTMLButtonElement;
    expect(action.disabled).toBe(true);
    expect(getByRole("dialog").textContent).toContain("workspace");
    expect(del).not.toHaveBeenCalled();
  });

  it("shows the daemon's 409 as the blocked note", async () => {
    vi.spyOn(api, "deleteProject").mockRejectedValue(new Error("project has tasks with a workspace"));
    const { getByRole, findByText } = render(<ProjectsView projects={[app]} onChanged={() => {}} />);
    fireEvent.click(getByRole("button", { name: /Más acciones de app/ }));
    fireEvent.click(getByRole("menuitem", { name: "Quitar…" }));
    fireEvent.click(getByRole("button", { name: "Quitar" }));
    await findByText("project has tasks with a workspace");
    expect((getByRole("button", { name: "Quitar" }) as HTMLButtonElement).disabled).toBe(true);
  });
});
