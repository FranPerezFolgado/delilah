// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { setLanguage } from "../i18n";
import { LogsView } from "./LogsView";

const LOGS = [
  "T1 DEBUG a: noisy",
  "T2 INFO b: started",
  "T3 WARN c: slow disk",
  "T4 ERROR d: boom",
  "  caused by: io",
].join("\n");

beforeEach(() => {
  setLanguage("es");
  vi.stubGlobal("fetch", vi.fn(async () => new Response(LOGS)));
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const lines = () => [...document.querySelectorAll(".ll")].map((e) => e.textContent?.trim());

it("shows all lines, level classes included", async () => {
  render(<LogsView onClose={() => {}} />);
  await waitFor(() => expect(lines().length).toBe(5));
  expect(document.querySelector(".ll-error")).toBeTruthy();
  expect(document.querySelector(".ll-warn")).toBeTruthy();
});
it("level filter: Advertencias keeps warn and error (and continuation)", async () => {
  render(<LogsView onClose={() => {}} />);
  await waitFor(() => expect(lines().length).toBe(5));
  fireEvent.click(screen.getByRole("radio", { name: "Advertencias" }));
  expect(lines()).toEqual(["T3 WARN c: slow disk", "T4 ERROR d: boom", "caused by: io"]);
  fireEvent.click(screen.getByRole("radio", { name: "Errores" }));
  expect(lines()).toEqual(["T4 ERROR d: boom", "caused by: io"]);
});
it("text filter, copy, close", async () => {
  const writeText = vi.fn(async () => {});
  vi.stubGlobal("navigator", { clipboard: { writeText } });
  const onClose = vi.fn();
  render(<LogsView onClose={onClose} />);
  await waitFor(() => expect(lines().length).toBe(5));
  fireEvent.change(screen.getByLabelText("Filtrar los logs"), { target: { value: "started" } });
  expect(lines()).toEqual(["T2 INFO b: started"]);
  fireEvent.click(screen.getByRole("button", { name: "Copiar" }));
  expect(writeText).toHaveBeenCalledWith("T2 INFO b: started");
  fireEvent.click(screen.getByRole("button", { name: "Cerrar" }));
  expect(onClose).toHaveBeenCalled();
});
it("titled dialog and an error when the daemon is down", async () => {
  vi.stubGlobal("fetch", vi.fn(async () => Promise.reject(new Error("x"))));
  render(<LogsView onClose={() => {}} />);
  expect(screen.getByRole("dialog", { name: "Logs de delilahd" })).toBeTruthy();
  await waitFor(() => expect(screen.getByRole("alert").textContent).toContain("No se pueden leer los logs"));
});
