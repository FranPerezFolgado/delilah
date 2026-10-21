// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { api, type Project, type Session } from "../api";
import { setLanguage } from "../i18n";
import { NewTaskDialog } from "./NewTaskDialog";

vi.mock("./WherePicker", async (orig) => ({ ...(await orig<typeof import("./WherePicker")>()), WherePicker: () => null }));

const web: Project = { name: "web", path: "/w/web", kind: "repo", workflow: "feature" };
const session = { id: "s9", task: "@web", kind: "claude", conversation: true } as Session;

function setup() {
  const props = { onClose: vi.fn(), onStarted: vi.fn(), onSaved: vi.fn(), onConversation: vi.fn() };
  const view = render(<NewTaskDialog projects={[web]} {...props} />);
  return { ...view, ...props };
}

beforeEach(() => setLanguage("es"));
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
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
