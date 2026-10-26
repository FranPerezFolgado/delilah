// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { Task, TaskStatus } from "../api";
import { setLanguage } from "../i18n";
import { StatusBar } from "./StatusBar";

beforeEach(() => setLanguage("es"));
afterEach(cleanup);

const task = (key: string, status: TaskStatus): Task => ({ key, title: key, project: null, workflow: "w", phase: null, status }) as Task;
const tasks = [
  task("A-1", { type: "running" }),
  task("A-2", { type: "running" }),
  task("A-3", { type: "waiting-for-user" } as TaskStatus),
  task("A-4", { type: "failed", reason: "x" } as TaskStatus),
  task("A-5", { type: "done" } as TaskStatus),
];
const noop = () => {};

it("counts tasks per group as buttons and drops «Agentes trabajando»", () => {
  render(<StatusBar status="up" tasks={tasks} usage={null} onFilter={noop} onLogs={noop} />);
  expect(screen.getByText("Conectado")).toBeTruthy();
  expect(screen.getByRole("button", { name: "2 trabajando" })).toBeTruthy();
  expect(screen.getByRole("button", { name: "1 te espera" })).toBeTruthy();
  expect(screen.getByRole("button", { name: "1 con error" }).className).toContain("sb-err");
  expect(screen.queryByRole("button", { name: /en pausa/ })).toBeNull();
  expect(document.body.textContent).not.toContain("Agentes trabajando");
});
it("clicking a count filters, the Logs button opens logs", () => {
  const onFilter = vi.fn();
  const onLogs = vi.fn();
  render(<StatusBar status="up" tasks={tasks} usage={null} onFilter={onFilter} onLogs={onLogs} />);
  fireEvent.click(screen.getByRole("button", { name: "1 con error" }));
  expect(onFilter).toHaveBeenCalledWith("err");
  fireEvent.click(screen.getByRole("button", { name: "Logs" }));
  expect(onLogs).toHaveBeenCalled();
});
it("shows usage windows", () => {
  const usage = { at: "", task: "", rate_limits: { five_hour: { used_percentage: 82 }, seven_day: { used_percentage: 35 } } };
  render(<StatusBar status="up" tasks={[]} usage={usage as never} onFilter={noop} onLogs={noop} />);
  expect(document.body.textContent).toContain("Claude 5 h 82 %");
  expect(document.body.textContent).toContain("Claude Semana 35 %");
});
it("disconnected: says so and hides counts", () => {
  render(<StatusBar status="down" tasks={tasks} usage={null} onFilter={noop} onLogs={noop} />);
  expect(screen.getByText("Sin conexión con delilahd · reintentando…")).toBeTruthy();
  expect(screen.queryByRole("button", { name: /trabajando/ })).toBeNull();
});
