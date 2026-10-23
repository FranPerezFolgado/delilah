// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { DEFAULT_PROJECT_SETTINGS, api, type Project, type Task } from "../api";
import { setLanguage } from "../i18n";
import { BacklogTaskView } from "./BacklogTaskView";

vi.mock("./WherePicker", async (orig) => ({ ...(await orig<typeof import("./WherePicker")>()), WherePicker: () => null }));

const web: Project = { name: "web", path: "/w/web", kind: "repo", workflow: "feature", settings: DEFAULT_PROJECT_SETTINGS };
const task: Task = {
  key: "LIBRE-1",
  title: "Algo",
  project: "web",
  workflow: "feature",
  phase: null,
  status: { type: "queued" },
  saving_mode: false,
  autopilot: null,
  handoff: null,
};

beforeEach(() => setLanguage("es"));
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const mount = (t: Task) => render(<BacklogTaskView task={t} projects={[web]} onStart={() => {}} onChanged={() => {}} />);

describe("BacklogTaskView autopilot", () => {
  it("ticking it ticks and disables saving mode, and saves both", async () => {
    const update = vi.spyOn(api, "updateTask").mockResolvedValue(task);
    const { getByLabelText, getByText } = mount(task);
    const saving = getByLabelText("Modo ahorro") as HTMLInputElement;
    fireEvent.click(getByLabelText("Autopiloto"));
    expect(saving.checked).toBe(true);
    expect(saving.disabled).toBe(true);
    fireEvent.click(getByText("Guardar"));
    await waitFor(() => expect(update).toHaveBeenCalled());
    expect(update.mock.calls[0][1]).toMatchObject({ autopilot: true, saving_mode: true });
  });

  it("unticking re-enables saving mode, still ticked", () => {
    const { getByLabelText } = mount({ ...task, saving_mode: true, autopilot: { state: "running", since: "2026-01-01T00:00:00Z" } });
    const auto = getByLabelText("Autopiloto") as HTMLInputElement;
    const saving = getByLabelText("Modo ahorro") as HTMLInputElement;
    expect(auto.checked).toBe(true);
    expect(saving.disabled).toBe(true);
    fireEvent.click(auto);
    expect(saving.disabled).toBe(false);
    expect(saving.checked).toBe(true);
  });

  it("is hidden for a task outside the spec workflow", () => {
    const { queryByLabelText } = mount({ ...task, workflow: "prompt" });
    expect(queryByLabelText("Autopiloto")).toBeNull();
    expect(queryByLabelText("Modo ahorro")).not.toBeNull();
  });
});
