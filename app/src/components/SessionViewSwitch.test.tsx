// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { api } from "../api";
import { setLanguage } from "../i18n";
import { SessionViewSwitch } from "./SessionViewSwitch";

beforeEach(() => setLanguage("es"));
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const sess = (runtime: "pty" | "stream", extra = {}) => ({ id: "s1", live: true, runtime, effective_runtime: runtime, ...extra });
const chatBtn = () => screen.getByRole("button", { name: /^Chat/ });
const termBtn = () => screen.getByRole("button", { name: "Terminal" });

it("a PTY session switches views without confirming", () => {
  const chooseView = vi.fn();
  const set = vi.spyOn(api, "setSessionRuntime");
  render(<SessionViewSwitch session={sess("pty")} view="terminal" chooseView={chooseView} />);
  expect(termBtn().getAttribute("aria-pressed")).toBe("true");
  fireEvent.click(chatBtn());
  expect(chooseView).toHaveBeenCalledWith("chat");
  expect(set).not.toHaveBeenCalled();
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
  await waitFor(() => expect(onError).toHaveBeenCalledWith("No se pudo cambiar: boom"));
});
