// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { api, ApiError } from "../api";
import { setLanguage } from "../i18n";
import { SessionViewSwitch, humanError } from "./SessionViewSwitch";

beforeEach(() => setLanguage("es"));
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const sess = (runtime: "pty" | "stream", extra = {}) => ({ id: "s1", live: true, runtime, effective_runtime: runtime, ...extra });
const chatBtn = () => screen.getByRole("button", { name: /^Chat/ });
const termBtn = () => screen.getByRole("button", { name: "Terminal" });

it("a PTY session asks before moving to chat and switches the runtime", async () => {
  const chooseView = vi.fn();
  const set = vi.spyOn(api, "setSessionRuntime").mockResolvedValue(null);
  const changed = vi.fn();
  render(<SessionViewSwitch session={sess("pty")} view="terminal" chooseView={chooseView} onChanged={changed} />);
  expect(termBtn().getAttribute("aria-pressed")).toBe("true");
  fireEvent.click(chatBtn());
  expect(set).not.toHaveBeenCalled();
  expect(chooseView).not.toHaveBeenCalled();
  expect(screen.getByText("Pasar a chat")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Cambiar" }));
  expect(set).toHaveBeenCalledWith("s1", "chat");
  await waitFor(() => expect(changed).toHaveBeenCalled());
});

it("humanError maps the daemon's runtime conflicts to Spanish and never shows English", () => {
  expect(humanError(new ApiError("wait for the turn to end or press Detener", 409))).toBe("Espera a que termine el turno");
  expect(humanError(new ApiError("only a task's Claude sessions switch runtime", 409))).toBe("Solo las sesiones de Claude de una tarea cambian de modo.");
  expect(humanError(new ApiError("the session already runs in chat", 409))).toBe("La sesión ya está en ese modo.");
  expect(humanError(new ApiError("Hay trabajo en segundo plano: espera a que termine o pulsa Detener", 409))).toBe("Hay trabajo en segundo plano: espera a que termine o pulsa Detener");
  expect(humanError(new ApiError("Background work is running: wait for it to finish or press Stop", 409))).toBe("Hay trabajo en segundo plano: espera a que termine o pulsa Detener");
  expect(humanError(new Error("boom"))).toBe("No se pudo completar la acción");
});

it("a stream session shows Chat and asks before moving to the terminal", async () => {
  const set = vi.spyOn(api, "setSessionRuntime").mockResolvedValue(null);
  const changed = vi.fn();
  render(<SessionViewSwitch session={sess("stream")} view="terminal" chooseView={vi.fn()} onChanged={changed} />);
  expect(chatBtn().getAttribute("aria-pressed")).toBe("true");
  fireEvent.click(termBtn());
  expect(set).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Cambiar" }));
  expect(set).toHaveBeenCalledWith("s1", "terminal");
  await waitFor(() => expect(changed).toHaveBeenCalled());
});

it("cancelling leaves the runtime alone", () => {
  const set = vi.spyOn(api, "setSessionRuntime");
  render(<SessionViewSwitch session={sess("stream")} view="chat" chooseView={vi.fn()} />);
  fireEvent.click(termBtn());
  fireEvent.click(screen.getByRole("button", { name: "Cancelar" }));
  expect(set).not.toHaveBeenCalled();
});

it("a stopped session follows effective_runtime", () => {
  render(<SessionViewSwitch session={{ id: "s1", live: false, runtime: "pty", effective_runtime: "stream" }} view="terminal" chooseView={vi.fn()} />);
  expect(chatBtn().getAttribute("aria-pressed")).toBe("true");
});

it("mid-turn both options are disabled with the reason", () => {
  render(<SessionViewSwitch session={sess("pty")} view="terminal" chooseView={vi.fn()} busy />);
  expect((termBtn() as HTMLButtonElement).disabled).toBe(true);
  expect(chatBtn().title).toBe("Espera a que termine el turno");
});

it("reports a failed switch", async () => {
  vi.spyOn(api, "setSessionRuntime").mockRejectedValue(new Error("boom"));
  const onError = vi.fn();
  render(<SessionViewSwitch session={sess("stream")} view="chat" chooseView={vi.fn()} onError={onError} />);
  fireEvent.click(termBtn());
  fireEvent.click(screen.getByRole("button", { name: "Cambiar" }));
  await waitFor(() => expect(onError).toHaveBeenCalledWith("No se pudo cambiar: No se pudo completar la acción"));
});
