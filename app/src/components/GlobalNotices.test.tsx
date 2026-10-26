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
  const { container } = render(<GlobalNotices tasks={[]} usage={usage(89)} current={null} onOpenTask={noop} />);
  expect(container.textContent).toBe("");
});

it("warns from 90 % with a tooltip", () => {
  render(<GlobalNotices tasks={[]} usage={usage(93)} current={null} onOpenTask={noop} />);
  const bar = screen.getByText("Claude 5 h al 93 %").parentElement!;
  expect(bar.getAttribute("title")).toBeTruthy();
});

it("announces the reached limit with the resume time and the paused tasks", () => {
  const at = Math.floor(new Date(2026, 0, 1, 14, 30).getTime() / 1000);
  const tasks = [task("A-1", { type: "paused-by-limit", resume_at: "" }), task("A-2", { type: "paused-by-limit", resume_at: "" })];
  render(<GlobalNotices tasks={tasks} usage={usage(100, at)} current={null} onOpenTask={noop} />);
  expect(document.body.textContent).toMatch(/Límite de 5 h alcanzado · se reanuda a las \d{1,2}[:.]30/);
  expect(screen.getByText("2 tareas en pausa")).toBeTruthy();
});

it("lists autopilot pauses with their reason and opens the task", () => {
  const onOpen = vi.fn();
  const ap = { state: "paused" as const, reason: "tests fallan", since: "" };
  render(<GlobalNotices tasks={[task("A-1", { type: "running" }, ap)]} usage={null} current={null} onOpenTask={onOpen} />);
  expect(screen.getByText("A-1 · autopiloto en pausa: tests fallan")).toBeTruthy();
  fireEvent.click(screen.getByText("Abrir"));
  expect(onOpen).toHaveBeenCalledWith("A-1");
});

it("toasts «te espera» when a task starts waiting elsewhere, not on first load or on its own screen", () => {
  const onOpen = vi.fn();
  const run = [task("A-1", { type: "running" }), task("A-2", { type: "running" })];
  const wait = (k: string) => task(k, { type: "waiting-for-user" } as TaskStatus);
  const ui = (tasks: Task[], current: string | null) => (
    <>
      <GlobalNotices tasks={tasks} usage={null} current={current} onOpenTask={onOpen} />
      <ToastHost />
    </>
  );
  const { rerender } = render(ui([wait("A-0"), ...run], null));
  expect(screen.queryByText("A-0 te espera")).toBeNull();
  act(() => rerender(ui([wait("A-0"), wait("A-1"), run[1]], "A-1")));
  expect(screen.queryByText("A-1 te espera")).toBeNull();
  act(() => rerender(ui([wait("A-0"), wait("A-1"), wait("A-2")], null)));
  expect(screen.getByText("A-2 te espera")).toBeTruthy();
  fireEvent.click(screen.getByText("Responder"));
  expect(onOpen).toHaveBeenCalledWith("A-2");
});

it("does not toast tasks already waiting when the list arrives after an empty first render", () => {
  const wait = (k: string) => task(k, { type: "waiting-for-user" } as TaskStatus);
  const ui = (tasks: Task[]) => (
    <>
      <GlobalNotices tasks={tasks} usage={null} current={null} onOpenTask={vi.fn()} />
      <ToastHost />
    </>
  );
  const { rerender } = render(ui([]));
  act(() => rerender(ui([wait("A-0"), wait("A-1")])));
  expect(screen.queryByText("A-0 te espera")).toBeNull();
  expect(screen.queryByText("A-1 te espera")).toBeNull();
});
