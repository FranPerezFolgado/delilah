// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, waitFor } from "@testing-library/react";
import type { EventRecord, Task } from "../api";
import { setLanguage, t } from "../i18n";
import { stepLabel } from "../chain";
import { TaskView } from "./TaskView";

const events = vi.fn<() => Promise<EventRecord[]>>();
vi.mock("../api", async (orig) => {
  const mod = await orig<typeof import("../api")>();
  return { ...mod, api: new Proxy({}, { get: (_, k) => (k === "events" ? events : () => new Promise(() => {})) }) };
});
vi.mock("./SpecPanel", () => ({ SpecPanel: () => null }));

const task: Task = { key: "PROJ-1", title: "A task", project: "web", workflow: "feature", phase: null, status: { type: "queued" }, saving_mode: false, title_source: "initial", previous_title: null, branch_error: null, handoff: null };

const show = () =>
  render(
    <TaskView task={task} sessions={[]} onOpenSession={() => {}} onNewSession={async () => {}} onChanged={() => {}} onViewChanges={() => {}} />,
  );

beforeEach(() => setLanguage("es"));
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  localStorage.clear();
});

describe("TaskView history", () => {
  it("labels a created-from-conversation event with the conversation label", async () => {
    events.mockResolvedValue([
      { at: "2026-01-01T10:00:00Z", event: { type: "created-from-conversation", session: "s1", label: "Idea X" } },
    ] as EventRecord[]);
    const { container } = show();
    await waitFor(() => expect(container.querySelector(".history")!.textContent).toContain("Creada desde la conversación «Idea X»"));
  });
});

describe("step-started history label", () => {
  it("names the step, not a phase", () => {
    const label = (step: "plan" | "implement" | "review") => t("Paso a {step} (sesión nueva)", { step: stepLabel(step) });
    expect(label("plan")).toBe("Paso a Spec y plan (sesión nueva)");
    expect(label("implement")).toBe("Paso a Implementación (sesión nueva)");
    expect(label("review")).toBe("Paso a Revisión (sesión nueva)");
  });

  it("translates the step labels", () => {
    setLanguage("en");
    expect(stepLabel("plan")).toBe("Spec and plan");
    expect(t("Paso a {step} (sesión nueva)", { step: stepLabel("review") })).toBe("Moved to Review (new session)");
  });
});
