// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, waitFor } from "@testing-library/react";
import type { EventRecord, Task } from "../api";
import { setLanguage } from "../i18n";
import { TaskView } from "./TaskView";

const events = vi.fn<() => Promise<EventRecord[]>>();
vi.mock("../api", async (orig) => {
  const mod = await orig<typeof import("../api")>();
  return { ...mod, api: new Proxy({}, { get: (_, k) => (k === "events" ? events : () => new Promise(() => {})) }) };
});
vi.mock("./SpecPanel", () => ({ SpecPanel: () => null }));

const task: Task = { key: "PROJ-1", title: "A task", project: "web", workflow: "feature", phase: null, status: { type: "queued" } };

const show = () =>
  render(
    <TaskView task={task} sessions={[]} onOpenSession={() => {}} onNewSession={async () => {}} onChanged={() => {}} onViewChanges={() => {}} />,
  );

beforeEach(() => setLanguage("es"));
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
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
