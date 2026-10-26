// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { setLanguage } from "../i18n";
import { DisconnectedBanner } from "./DisconnectedBanner";

beforeEach(() => setLanguage("es"));
afterEach(cleanup);

it("says it is retrying, how to start it, and retries on click", () => {
  const onRetry = vi.fn();
  render(<DisconnectedBanner lastOk={null} onRetry={onRetry} />);
  expect(screen.getByRole("alert").textContent).toContain("Sin conexión con delilahd · reintentando…");
  expect(screen.getByRole("alert").textContent).toContain("delilahd");
  expect(document.body.textContent).not.toContain("Datos de");
  fireEvent.click(screen.getByRole("button", { name: "Reintentar ahora" }));
  expect(onRetry).toHaveBeenCalled();
});
it("shows how old the data is", () => {
  render(<DisconnectedBanner lastOk={Date.now() - 4 * 60_000} onRetry={() => {}} />);
  expect(screen.getByRole("alert").textContent).toMatch(/Datos de hace 4 min/);
});
