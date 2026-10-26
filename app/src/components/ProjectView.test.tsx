// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { setLanguage } from "../i18n";
import { DEFAULT_PROJECT_SETTINGS, api, type Project, type Session, type Task } from "../api";
import { ProjectView } from "./ProjectView";

beforeEach(() => {
  setLanguage("es");
  vi.spyOn(api, "file").mockRejectedValue(new Error("none"));
  vi.spyOn(api, "tree").mockResolvedValue([]);
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const project: Project = { name: "app", path: "/x", kind: "repo", workflow: "feature", settings: DEFAULT_PROJECT_SETTINGS, preview_command: "npm run dev" };
const session = (over: Partial<Session>): Session => ({
  id: "s1", task: "@app", kind: "claude", label: "Auth", primary: false, closed: false, live: true, read_only: true,
  state: "idle", conversation: false, step: null, ...over,
} as Session);
const task = (over: Partial<Task> & { key: string }): Task => ({
  title: "T", project: "app", workflow: "feature", phase: null, status: { type: "running" }, ...over,
} as Task);

const view = (p: Project = project, extra: Partial<React.ComponentProps<typeof ProjectView>> = {}) =>
  render(<ProjectView project={p} sessions={[]} onOpenSession={() => {}} onNewSession={async () => {}} {...extra} />);

describe("ProjectView header and actions", () => {
  it("shows the path once and the three actions", () => {
    const onNewSession = vi.fn(async () => {});
    const onNewTask = vi.fn();
    const { container, getByRole } = view(project, { onNewSession, onNewTask });
    expect(container.textContent!.split("/x").length - 1).toBe(1);
    fireEvent.click(getByRole("button", { name: /Preguntar a Claude/ }));
    expect(onNewSession).toHaveBeenCalledWith("claude");
    fireEvent.click(getByRole("button", { name: /Terminal/ }));
    expect(onNewSession).toHaveBeenCalledWith("shell");
    fireEvent.click(getByRole("button", { name: /Nueva tarea/ }));
    expect(onNewTask).toHaveBeenCalled();
  });
});

describe("ProjectView lists", () => {
  it("splits conversations from ideas, with last message and opens one", () => {
    const onOpenSession = vi.fn();
    const { getByText, getByLabelText } = view(project, {
      onOpenSession,
      sessions: [
        session({ id: "a", label: "Auth", last_message: "Mira el login" }),
        session({ id: "b", label: "Idea X", conversation: true }),
      ],
    });
    expect(getByLabelText("Conversaciones").textContent).toContain("Mira el login");
    expect(getByLabelText("Conversaciones").textContent).not.toContain("Idea X");
    expect(getByLabelText("Ideas").textContent).toContain("Idea X");
    fireEvent.click(getByText("Auth"));
    expect(onOpenSession).toHaveBeenCalledWith("a");
  });

  it("lists only this project's active tasks", () => {
    const onOpenTask = vi.fn();
    const { getByText, queryByText } = view(project, {
      onOpenTask,
      tasks: [
        task({ key: "T-1", title: "Activa" }),
        task({ key: "T-2", title: "Hecha", status: { type: "done" } as Task["status"] }),
        task({ key: "T-3", title: "Otra", project: "other" }),
      ],
    });
    fireEvent.click(getByText("Activa"));
    expect(onOpenTask).toHaveBeenCalledWith("T-1");
    expect(queryByText("Hecha")).toBeNull();
    expect(queryByText("Otra")).toBeNull();
  });
});

describe("ProjectView README and tree", () => {
  it("reads both through the @project owner", async () => {
    (api.file as ReturnType<typeof vi.fn>).mockResolvedValue({ path: "README.md", text: "# Hola mundo", version: "1", size: 1 });
    (api.tree as ReturnType<typeof vi.fn>).mockResolvedValue([{ name: "src", dir: true }, { name: "Cargo.toml", dir: false }]);
    const { findByText } = view();
    await findByText("Hola mundo");
    await findByText("Cargo.toml");
    expect(api.file).toHaveBeenCalledWith("@app", "README.md");
    expect(api.tree).toHaveBeenCalledWith("@app", "");
  });
});

describe("ProjectView preview command", () => {
  it("saves a changed command with Guardar", async () => {
    const update = vi.spyOn(api, "updateProject").mockResolvedValue(project);
    const { getByLabelText, getByRole } = view();
    const input = getByLabelText("Comando de preview") as HTMLInputElement;
    expect(input.value).toBe("npm run dev");
    fireEvent.change(input, { target: { value: " pnpm dev " } });
    fireEvent.click(getByRole("button", { name: "Guardar" }));
    await waitFor(() => expect(update).toHaveBeenCalledWith("app", { preview_command: "pnpm dev" }));
  });

  it("clears it when emptied, on Enter", async () => {
    const update = vi.spyOn(api, "updateProject").mockResolvedValue(project);
    const { getByLabelText } = view();
    const input = getByLabelText("Comando de preview");
    fireEvent.change(input, { target: { value: "" } });
    fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => expect(update).toHaveBeenCalledWith("app", { preview_command: null }));
  });

  it("keeps Guardar disabled while unchanged and has help text", () => {
    const update = vi.spyOn(api, "updateProject").mockResolvedValue(project);
    const { getByLabelText, getByRole } = view();
    const save = getByRole("button", { name: "Guardar" }) as HTMLButtonElement;
    expect(save.disabled).toBe(true);
    fireEvent.click(save);
    expect(update).not.toHaveBeenCalled();
    expect(getByLabelText("Comando de preview").getAttribute("aria-describedby")).toBe("prj-preview-help");
  });

  it("shows the error when saving fails", async () => {
    vi.spyOn(api, "updateProject").mockRejectedValue(new Error("no se pudo guardar"));
    const { getByLabelText, getByRole, findByText } = view();
    fireEvent.change(getByLabelText("Comando de preview"), { target: { value: "pnpm dev" } });
    fireEvent.click(getByRole("button", { name: "Guardar" }));
    await findByText("no se pudo guardar");
  });
});
