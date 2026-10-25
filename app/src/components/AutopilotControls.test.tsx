// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { setLanguage } from "../i18n";
import type { Autopilot } from "../api";
import { AutopilotControls } from "./AutopilotControls";

beforeEach(() => setLanguage("es"));
afterEach(cleanup);

const ap = (over: Partial<Autopilot>): Autopilot => ({ state: "running", since: "2026-01-01T00:00:00Z", ...over });
const setup = (autopilot: Autopilot | null, saving = false, extra = {}) => {
  const onAutopilot = vi.fn();
  const onSavingMode = vi.fn();
  render(<AutopilotControls autopilot={autopilot} savingMode={saving} onAutopilot={onAutopilot} onSavingMode={onSavingMode} {...extra} />);
  return { onAutopilot, onSavingMode };
};

it("turning autopilot on asks first, then calls on", () => {
  const { onAutopilot } = setup(null);
  fireEvent.click(screen.getByRole("button", { name: "Autopiloto" }));
  expect(onAutopilot).not.toHaveBeenCalled();
  expect(screen.getByRole("dialog")).not.toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Activar" }));
  expect(onAutopilot).toHaveBeenCalledWith("on");
});

it("cancelling the confirmation does nothing", () => {
  const { onAutopilot } = setup(null);
  fireEvent.click(screen.getByRole("button", { name: "Autopiloto" }));
  fireEvent.click(screen.getByRole("button", { name: "Cancelar" }));
  expect(onAutopilot).not.toHaveBeenCalled();
  expect(screen.queryByRole("dialog")).toBeNull();
});

it("turning it off needs no confirmation", () => {
  const { onAutopilot } = setup(ap({}));
  fireEvent.click(screen.getByRole("button", { name: "Autopiloto" }));
  expect(onAutopilot).toHaveBeenCalledWith("off");
});

it("while on, Ahorro is forced on and disabled with the reason", () => {
  setup(ap({}), false);
  const b = screen.getByRole("button", { name: "Ahorro" }) as HTMLButtonElement;
  expect(b.getAttribute("aria-pressed")).toBe("true");
  expect(b.disabled).toBe(true);
  expect(b.title).toContain("autopiloto");
});

it("Ahorro toggles when autopilot is off", () => {
  const { onSavingMode } = setup(null, false);
  fireEvent.click(screen.getByRole("button", { name: "Ahorro" }));
  expect(onSavingMode).toHaveBeenCalledWith(true);
});

it("paused shows the label, the reason and resumes", () => {
  const { onAutopilot } = setup(ap({ state: "paused", reason: "límite de 5 h · se reanuda a las 18:40" }));
  expect(screen.getByText("límite de 5 h · se reanuda a las 18:40")).not.toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Autopiloto en pausa · Reanudar" }));
  expect(onAutopilot).toHaveBeenCalledWith("resume");
});

it("iconOnly keeps the names accessible without visible labels", () => {
  setup(null, false, { iconOnly: true });
  const b = screen.getByRole("button", { name: "Ahorro" });
  expect(b.textContent).toBe("");
});
