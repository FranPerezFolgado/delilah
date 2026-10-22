// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { api, type Project, type Session, type Task } from "../api";
import { setLanguage } from "../i18n";
import { NewTaskDialog } from "./NewTaskDialog";

vi.mock("./WherePicker", async (orig) => ({ ...(await orig<typeof import("./WherePicker")>()), WherePicker: () => null }));

const web: Project = { name: "web", path: "/w/web", kind: "repo", workflow: "feature" };
const session = { id: "s9", task: "@web", kind: "claude", conversation: true } as Session;

function setup() {
  vi.spyOn(api, "preference").mockResolvedValue(null);
  const props = { onClose: vi.fn(), onStarted: vi.fn(), onSaved: vi.fn(), onConversation: vi.fn() };
  const view = render(<NewTaskDialog projects={[web]} {...props} />);
  return { ...view, ...props };
}

beforeEach(() => {
  setLanguage("es");
  vi.spyOn(api, "submodules").mockResolvedValue([]);
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  localStorage.clear();
});

describe("NewTaskDialog origins", () => {
  it("Con Claude opens a conversation with the idea and hides the task fields", async () => {
    const open = vi.spyOn(api, "openProjectSession").mockResolvedValue(session);
    const { getByText, getByLabelText, queryByPlaceholderText, container, onConversation } = setup();
    fireEvent.click(getByText("Con Claude"));
    expect(queryByPlaceholderText("PROJ-1234")).toBeNull();
    fireEvent.change(getByLabelText("Tu idea"), { target: { value: "cache rates" } });
    fireEvent.submit(container.querySelector("form")!);
    await waitFor(() => expect(onConversation).toHaveBeenCalledWith("web", session));
    expect(open).toHaveBeenCalledWith("web", { kind: "claude", conversation: true, prompt: "cache rates" });
  });

  it("Con Claude without an idea sends no prompt", async () => {
    const open = vi.spyOn(api, "openProjectSession").mockResolvedValue(session);
    const { getByText, container, onConversation } = setup();
    fireEvent.click(getByText("Con Claude"));
    fireEvent.submit(container.querySelector("form")!);
    await waitFor(() => expect(onConversation).toHaveBeenCalled());
    expect(open).toHaveBeenCalledWith("web", { kind: "claude", conversation: true, prompt: undefined });
  });

  it("the Jira path still runs a task", async () => {
    const task = { key: "PROJ-1" } as never;
    const run = vi.spyOn(api, "run").mockResolvedValue(task);
    const { getByPlaceholderText, container, onStarted } = setup();
    fireEvent.change(getByPlaceholderText("PROJ-1234"), { target: { value: "PROJ-1" } });
    fireEvent.change(container.querySelector("input:not(.mono)")!, { target: { value: "Fix" } });
    fireEvent.submit(container.querySelector("form")!);
    await waitFor(() => expect(onStarted).toHaveBeenCalledWith(task));
    expect(run.mock.calls[0][0]).toMatchObject({ key: "PROJ-1", title: "Fix", free: false });
  });

  it("the Libre path still runs a free task", async () => {
    const task = { key: "LIBRE-1" } as never;
    const run = vi.spyOn(api, "run").mockResolvedValue(task);
    const { getByText, getByPlaceholderText, container, onStarted } = setup();
    fireEvent.click(getByText("Tarea libre"));
    fireEvent.change(getByPlaceholderText(/caché de tarifas/), { target: { value: "Do it" } });
    fireEvent.submit(container.querySelector("form")!);
    await waitFor(() => expect(onStarted).toHaveBeenCalledWith(task));
    expect(run.mock.calls[0][0]).toMatchObject({ key: "", title: "Do it", free: true });
  });
});

const projects: Project[] = [{ name: "web", path: "/p/web", kind: "repo", workflow: "feature" }];
const started: Task = {
  key: "LIBRE-1",
  title: "t",
  project: "web",
  workflow: "feature",
  phase: null,
  status: { type: "queued" },
  saving_mode: true,
  autopilot: null,
  handoff: null,
};

describe("NewTaskDialog saving mode", () => {
  it("is preloaded from the saving-mode preference and sent with the run", async () => {
    vi.spyOn(api, "preference").mockResolvedValue(true);
    const run = vi.spyOn(api, "run").mockResolvedValueOnce(started);
    const { getByLabelText, container } = render(
      <NewTaskDialog projects={projects} draft={{ key: "PROJ-1", title: "Algo", components: [] }} onClose={() => {}} onStarted={() => {}} onSaved={() => {}} onConversation={() => {}} />,
    );
    await waitFor(() => expect((getByLabelText("Modo ahorro") as HTMLInputElement).checked).toBe(true));
    fireEvent.submit(container.querySelector("form")!);
    await waitFor(() => expect(run).toHaveBeenCalled());
    expect(run.mock.calls[0][0].saving_mode).toBe(true);
  });

  it("defaults to off and can be switched on, also for the backlog", async () => {
    vi.spyOn(api, "preference").mockResolvedValue(null);
    const create = vi.spyOn(api, "createTask").mockResolvedValueOnce(started);
    const { getByLabelText, getByText } = render(
      <NewTaskDialog projects={projects} draft={{ key: "PROJ-1", title: "Algo", components: [] }} onClose={() => {}} onStarted={() => {}} onSaved={() => {}} onConversation={() => {}} />,
    );
    const box = getByLabelText("Modo ahorro") as HTMLInputElement;
    expect(box.checked).toBe(false);
    fireEvent.click(box);
    fireEvent.click(getByText("Guardar en backlog"));
    await waitFor(() => expect(create).toHaveBeenCalled());
    expect(create.mock.calls[0][0].saving_mode).toBe(true);
  });

  it("starts a backlog task with the mode it was saved with", async () => {
    const pref = vi.spyOn(api, "preference").mockResolvedValue(false);
    const { getByLabelText } = render(
      <NewTaskDialog projects={projects} existing={{ ...started, saving_mode: true }} onClose={() => {}} onStarted={() => {}} onSaved={() => {}} onConversation={() => {}} />,
    );
    await Promise.resolve();
    expect((getByLabelText("Modo ahorro") as HTMLInputElement).checked).toBe(true);
    expect(pref).not.toHaveBeenCalled();
  });
});

describe("NewTaskDialog autopilot", () => {
  const draft = { key: "PROJ-1", title: "Algo", components: [] };
  const mount = () =>
    render(<NewTaskDialog projects={projects} draft={draft} onClose={() => {}} onStarted={() => {}} onSaved={() => {}} onConversation={() => {}} />);

  it("ticking it ticks and disables saving mode; unticking re-enables it, still ticked", async () => {
    vi.spyOn(api, "preference").mockResolvedValue(null);
    const { getByLabelText } = mount();
    const auto = getByLabelText("Autopiloto") as HTMLInputElement;
    const saving = getByLabelText("Modo ahorro") as HTMLInputElement;
    fireEvent.click(auto);
    expect(saving.checked).toBe(true);
    expect(saving.disabled).toBe(true);
    fireEvent.click(auto);
    expect(saving.disabled).toBe(false);
    expect(saving.checked).toBe(true);
  });

  it("is sent with the run together with saving mode", async () => {
    vi.spyOn(api, "preference").mockResolvedValue(null);
    const run = vi.spyOn(api, "run").mockResolvedValueOnce(started);
    const { getByLabelText, container } = mount();
    fireEvent.click(getByLabelText("Autopiloto"));
    fireEvent.submit(container.querySelector("form")!);
    await waitFor(() => expect(run).toHaveBeenCalled());
    expect(run.mock.calls[0][0].autopilot).toBe(true);
    expect(run.mock.calls[0][0].saving_mode).toBe(true);
  });

  it("is sent when saving to the backlog", async () => {
    vi.spyOn(api, "preference").mockResolvedValue(null);
    const create = vi.spyOn(api, "createTask").mockResolvedValueOnce(started);
    const { getByLabelText, getByText } = mount();
    fireEvent.click(getByLabelText("Autopiloto"));
    fireEvent.click(getByText("Guardar en backlog"));
    await waitFor(() => expect(create).toHaveBeenCalled());
    expect(create.mock.calls[0][0]).toMatchObject({ autopilot: true, saving_mode: true });
  });

  it("unticking it on a backlog task that has it sends autopilot: false", async () => {
    vi.spyOn(api, "preference").mockResolvedValue(null);
    const run = vi.spyOn(api, "run").mockResolvedValueOnce(started);
    const existing = { ...started, autopilot: { state: "running" as const, since: "2026-10-21T10:00:00Z" } };
    const { getByLabelText, container } = render(
      <NewTaskDialog projects={projects} existing={existing} onClose={() => {}} onStarted={() => {}} onSaved={() => {}} onConversation={() => {}} />,
    );
    fireEvent.click(getByLabelText("Autopiloto"));
    fireEvent.submit(container.querySelector("form")!);
    await waitFor(() => expect(run).toHaveBeenCalled());
    expect(run.mock.calls[0][0].autopilot).toBe(false);
  });

  it("is hidden outside the spec workflow", () => {
    vi.spyOn(api, "preference").mockResolvedValue(null);
    const { queryByLabelText } = render(
      <NewTaskDialog projects={[{ ...web, workflow: "custom" }]} draft={draft} onClose={() => {}} onStarted={() => {}} onSaved={() => {}} onConversation={() => {}} />,
    );
    expect(queryByLabelText("Autopiloto")).toBeNull();
  });
});
