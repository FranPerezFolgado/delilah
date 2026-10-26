// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { Task, TaskStatus, UsageReport } from "../api";
import { setLanguage } from "../i18n";
import { GlobalNotices } from "./GlobalNotices";
import { ToastHost, clearToasts } from "./ui";

beforeEach(() => setLanguage("es"));
afterEach(() => {
  cleanup();
  clearToasts();
});

const task = (key: string, status: TaskStatus, autopilot?: Task["autopilot"]): Task =>
  ({ key, title: key, project: null, workflow: "w", phase: null, status, autopilot }) as Task;
const usage = (five: number, resets_at?: number): UsageReport => ({
  at: "",
  task: "",
  rate_limits: { five_hour: { used_percentage: five, resets_at } },
});
const noop = () => {};

it("shows nothing below 90 %", () => {
  const { container } = render(<GlobalNotices tasks={[]} usage={usage(89)} current={null} onOpenTask={noop} onOpenInbox={noop} />);
  expect(container.textContent).toBe("");
});

it("warns from 90 % with a tooltip", () => {
  render(<GlobalNotices tasks={[]} usage={usage(93)} current={null} onOpenTask={noop} onOpenInbox={noop} />);
  const bar = screen.getByText("Claude 5 h al 93 %").parentElement!;
  expect(bar.getAttribute("title")).toBeTruthy();
});

it("announces the reached limit with the resume time and the paused tasks", () => {
  const at = Math.floor(new Date(2026, 0, 1, 14, 30).getTime() / 1000);
  const tasks = [task("A-1", { type: "paused-by-limit", resume_at: "" }), task("A-2", { type: "paused-by-limit", resume_at: "" })];
  render(<GlobalNotices tasks={tasks} usage={usage(100, at)} current={null} onOpenTask={noop} onOpenInbox={noop} />);
  expect(document.body.textContent).toMatch(/Límite de 5 h alcanzado · se reanuda a las \d{1,2}[:.]30/);
  expect(screen.getByText("2 tareas en pausa")).toBeTruthy();
});

it("lists autopilot pauses with their reason and opens the task", () => {
  const onOpen = vi.fn();
  const ap = { state: "paused" as const, reason: "tests fallan", since: "" };
  render(<GlobalNotices tasks={[task("A-1", { type: "running" }, ap)]} usage={null} current={null} onOpenTask={onOpen} onOpenInbox={noop} />);
  expect(screen.getByText("A-1 · autopiloto en pausa: tests fallan")).toBeTruthy();
  fireEvent.click(screen.getByText("Abrir"));
  expect(onOpen).toHaveBeenCalledWith("A-1");
});

const wait = (k: string) => task(k, { type: "waiting-for-user" } as TaskStatus);
const idle = (k: string) => task(k, { type: "running" });
function setup() {
  const onOpen = vi.fn();
  const onInbox = vi.fn();
  const ui = (tasks: Task[], current: string | null = null) => (
    <>
      <GlobalNotices tasks={tasks} usage={null} current={current} onOpenTask={onOpen} onOpenInbox={onInbox} />
      <ToastHost />
    </>
  );
  const view = render(ui([idle("A-0"), idle("A-1")]));
  const to = (tasks: Task[], current: string | null = null) => act(() => view.rerender(ui(tasks, current)));
  return { onOpen, onInbox, to, ...view, ui };
}

it("groups «te espera» toasts: one task replies, two go to the inbox, none removes it", () => {
  const { onOpen, onInbox, to } = setup();
  to([wait("A-0"), idle("A-1")]);
  expect(screen.getByText("A-0 te espera")).toBeTruthy();
  fireEvent.click(screen.getByText("Responder"));
  expect(onOpen).toHaveBeenCalledWith("A-0");
  to([wait("A-0"), wait("A-1")]);
  expect(screen.queryByText("A-0 te espera")).toBeNull();
  expect(screen.getByText("2 tareas te esperan")).toBeTruthy();
  expect(screen.getAllByText("Ver")).toHaveLength(1);
  fireEvent.click(screen.getByText("Ver"));
  expect(onInbox).toHaveBeenCalled();
  to([idle("A-0"), wait("A-1")]);
  expect(screen.getByText("A-1 te espera")).toBeTruthy();
  to([idle("A-0"), idle("A-1")]);
  expect(document.body.textContent).not.toMatch(/te espera|te esperan/);
});

it("removes a task that leaves the list or becomes current", () => {
  const { to } = setup();
  to([wait("A-0"), wait("A-1")]);
  expect(screen.getByText("2 tareas te esperan")).toBeTruthy();
  to([wait("A-0")]);
  expect(screen.getByText("A-0 te espera")).toBeTruthy();
  to([wait("A-0"), wait("A-1")]);
  to([wait("A-0"), wait("A-1")], "A-1");
  expect(screen.getByText("A-0 te espera")).toBeTruthy();
});

it("never adds the current task", () => {
  const { to } = setup();
  to([idle("A-0"), wait("A-1")], "A-1");
  expect(document.body.textContent).not.toMatch(/te espera/);
});

it("closing the group keeps it closed until a new task waits, then only that one shows", () => {
  const { to } = setup();
  to([wait("A-0"), idle("A-1")]);
  fireEvent.click(screen.getByLabelText("Cerrar"));
  expect(document.body.textContent).not.toMatch(/te espera/);
  to([wait("A-0"), idle("A-1")]);
  expect(document.body.textContent).not.toMatch(/te espera/);
  to([wait("A-0"), wait("A-1")]);
  expect(screen.getByText("A-1 te espera")).toBeTruthy();
});

it("removes the toast on unmount", () => {
  const { to, unmount } = setup();
  to([wait("A-0"), idle("A-1")]);
  unmount();
  // Remount only the host: the toast must be gone.
  render(<ToastHost />);
  expect(screen.queryByText("A-0 te espera")).toBeNull();
});

it("does not toast tasks already waiting when the list arrives after an empty first render", () => {
  const wait = (k: string) => task(k, { type: "waiting-for-user" } as TaskStatus);
  const ui = (tasks: Task[]) => (
    <>
      <GlobalNotices tasks={tasks} usage={null} current={null} onOpenTask={vi.fn()} onOpenInbox={noop} />
      <ToastHost />
    </>
  );
  const { rerender } = render(ui([]));
  act(() => rerender(ui([wait("A-0"), wait("A-1")])));
  expect(screen.queryByText("A-0 te espera")).toBeNull();
  expect(screen.queryByText("A-1 te espera")).toBeNull();
});
