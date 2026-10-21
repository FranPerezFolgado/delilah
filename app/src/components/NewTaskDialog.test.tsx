// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { api, type Project, type Task } from "../api";
import { setLanguage } from "../i18n";
import { NewTaskDialog } from "./NewTaskDialog";

const projects: Project[] = [{ name: "web", path: "/p/web", kind: "repo", workflow: "feature" }];
const started: Task = {
  key: "LIBRE-1",
  title: "t",
  project: "web",
  workflow: "feature",
  phase: null,
  status: { type: "queued" },
  saving_mode: true,
  handoff: null,
};

beforeEach(() => {
  setLanguage("es");
  vi.spyOn(api, "submodules").mockResolvedValue([]);
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  localStorage.clear();
});

describe("NewTaskDialog saving mode", () => {
  it("is preloaded from the saving-mode preference and sent with the run", async () => {
    vi.spyOn(api, "preference").mockResolvedValue(true);
    const run = vi.spyOn(api, "run").mockResolvedValueOnce(started);
    const { getByLabelText, container } = render(
      <NewTaskDialog projects={projects} draft={{ key: "PROJ-1", title: "Algo", components: [] }} onClose={() => {}} onStarted={() => {}} onSaved={() => {}} />,
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
      <NewTaskDialog projects={projects} draft={{ key: "PROJ-1", title: "Algo", components: [] }} onClose={() => {}} onStarted={() => {}} onSaved={() => {}} />,
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
      <NewTaskDialog projects={projects} existing={{ ...started, saving_mode: true }} onClose={() => {}} onStarted={() => {}} onSaved={() => {}} />,
    );
    await Promise.resolve();
    expect((getByLabelText("Modo ahorro") as HTMLInputElement).checked).toBe(true);
    expect(pref).not.toHaveBeenCalled();
  });
});
