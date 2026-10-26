// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { setLanguage } from "../i18n";
import { api, type Project, type Task } from "../api";
import { FirstRun } from "./FirstRun";

beforeEach(() => setLanguage("es"));
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const project = { name: "p" } as Project;
const task = { key: "A-1" } as Task;
const mock = (saved: unknown, jira: unknown = null) => {
  vi.spyOn(api, "preference").mockResolvedValue(saved as never);
  vi.spyOn(api, "jira").mockResolvedValue({ settings: jira as never });
  return vi.spyOn(api, "setPreference").mockResolvedValue(null);
};
const setup = (projects: Project[] = [], tasks: Task[] = []) =>
  render(<FirstRun projects={projects} tasks={tasks} onAddProject={() => {}} onConnectJira={() => {}} onNewTask={() => {}} />);

it("lists the three steps", async () => {
  mock(null);
  setup();
  await screen.findByText("Primeros pasos");
  expect(screen.getAllByRole("listitem").length).toBe(3);
  expect(screen.getByRole("button", { name: "Omitir" })).not.toBeNull();
});

it("marks the steps that are done", async () => {
  mock(null);
  setup([project]);
  await screen.findByText("Primeros pasos");
  const items = screen.getAllByRole("listitem");
  expect(items[0].className).toBe("done");
  expect(items[2].className).not.toBe("done");
});

it("Omitir saves the skip and marks Jira", async () => {
  const set = mock(null);
  setup();
  fireEvent.click(await screen.findByRole("button", { name: "Omitir" }));
  expect(set).toHaveBeenCalledWith("first-run", { jiraSkipped: true });
  expect(screen.queryByRole("button", { name: "Omitir" })).toBeNull();
  expect(screen.getByText("Jira omitido")).not.toBeNull();
});

it("a remembered skip stays skipped", async () => {
  mock({ jiraSkipped: true });
  setup();
  await screen.findByText("Jira omitido");
});

it("hides once the three steps are done", async () => {
  mock({ jiraSkipped: true });
  setup([project], [task]);
  await waitFor(() => expect(screen.queryByText("Primeros pasos")).toBeNull());
});

it("Jira connected counts as done", async () => {
  mock(null, { mode: "cloud" });
  setup([project], [task]);
  await waitFor(() => expect(screen.queryByText("Primeros pasos")).toBeNull());
});
