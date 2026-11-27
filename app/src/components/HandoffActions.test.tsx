// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/react";
import { api, type Autopilot } from "../api";
import { setLanguage } from "../i18n";
import { HandoffActions } from "./HandoffActions";

describe("HandoffActions", () => {
  beforeEach(() => setLanguage("es"));
  afterEach(cleanup);

  it("shows the start without buttons", () => {
    const { getByText, queryAllByRole } = render(
      <HandoffActions task="A-1" handoff={{ to: "review", stage: "starting" }} />,
    );
    expect(getByText("Empezando la sesión nueva…")).toBeTruthy();
    expect(queryAllByRole("button")).toHaveLength(0);
  });
});

describe("HandoffActions paused banner", () => {
  beforeEach(() => setLanguage("es"));
  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });
  const paused = (reason: string, since: string): Autopilot => ({ state: "paused", reason, since });

  it("has the X; it hides the banner with no resume/off call and keeps the buttons", () => {
    const set = vi.spyOn(api, "setAutopilot").mockResolvedValue(undefined as never);
    const { container, getByRole, getByText } = render(
      <HandoffActions task="A-1" handoff={null} autopilot={paused("r", "2026-01-01T00:00:00Z")} />,
    );
    expect(getByText("Reanudar autopiloto")).toBeTruthy();
    expect(getByText("Seguir a mano")).toBeTruthy();
    expect(container.querySelectorAll(".ntc-x")).toHaveLength(1);
    fireEvent.click(getByRole("button", { name: "Cerrar" }));
    expect(container.querySelector(".ha-banner")).toBeNull();
    expect(set).not.toHaveBeenCalled();
  });

  it("shows again with a new reason or since, and after leaving the pause", () => {
    const a = paused("r", "2026-01-01T00:00:00Z");
    const { container, getByRole, rerender } = render(<HandoffActions task="A-1" handoff={null} autopilot={a} />);
    fireEvent.click(getByRole("button", { name: "Cerrar" }));
    rerender(<HandoffActions task="A-1" handoff={null} autopilot={{ ...a }} />);
    expect(container.querySelector(".ha-banner")).toBeNull();
    rerender(<HandoffActions task="A-1" handoff={null} autopilot={paused("other", a.since)} />);
    expect(container.querySelector(".ha-banner")).not.toBeNull();
    fireEvent.click(getByRole("button", { name: "Cerrar" }));
    rerender(<HandoffActions task="A-1" handoff={null} autopilot={paused("other", "2026-02-01T00:00:00Z")} />);
    expect(container.querySelector(".ha-banner")).not.toBeNull();
    fireEvent.click(getByRole("button", { name: "Cerrar" }));
    rerender(<HandoffActions task="A-1" handoff={null} autopilot={{ ...a, state: "running" }} />);
    rerender(<HandoffActions task="A-1" handoff={null} autopilot={paused("other", "2026-02-01T00:00:00Z")} />);
    expect(container.querySelector(".ha-banner")).not.toBeNull();
  });
});

describe("HandoffActions offer", () => {
  beforeEach(() => setLanguage("en"));
  afterEach(cleanup);

  it("uses the kit buttons: primary to implement, secondary to stay", () => {
    const { getByText } = render(<HandoffActions task="A-1" handoff={{ to: "review", stage: "offered" }} />);
    expect(getByText("Implement in a new session").closest("button")!.className).toContain("b-p");
    expect(getByText("Keep going here").closest("button")!.className).toContain("b-s");
  });
});
