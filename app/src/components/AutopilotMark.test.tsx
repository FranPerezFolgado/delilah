// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { api, type Autopilot } from "../api";
import { setLanguage } from "../i18n";
import { AutopilotMark } from "./AutopilotMark";
import { HandoffActions } from "./HandoffActions";

beforeEach(() => setLanguage("es"));

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const ap = (over: Partial<Autopilot>): Autopilot => ({ state: "running", since: "2026-01-01T00:00:00Z", ...over });

describe("AutopilotMark", () => {
  it("shows Autopiloto while running", () => {
    render(<AutopilotMark autopilot={ap({})} />);
    expect(screen.getByText("Autopiloto")).not.toBeNull();
  });

  it("shows the pause and its reason in the tooltip", () => {
    const { container } = render(<AutopilotMark autopilot={ap({ state: "paused", reason: "Fallaron los tests" })} />);
    expect(screen.getByText("Autopiloto en pausa")).not.toBeNull();
    expect(container.querySelector(".autopilot-pill")!.getAttribute("title")).toContain("Fallaron los tests");
  });

  it("says since when it is paused, in the tooltip", () => {
    const { container } = render(<AutopilotMark autopilot={ap({ state: "paused", reason: "x", since: new Date(Date.now() - 5 * 60000).toISOString() })} />);
    expect(container.querySelector(".autopilot-pill")!.getAttribute("title")).toContain("desde hace 5 min");
  });

  it("shows the PR opened by autopilot when done", () => {
    render(<AutopilotMark autopilot={ap({ state: "done" })} />);
    expect(screen.getByText("PR abierta por autopiloto")).not.toBeNull();
  });

  it("renders nothing without autopilot", () => {
    const { container } = render(<AutopilotMark autopilot={null} />);
    expect(container.innerHTML).toBe("");
  });
});

describe("paused autopilot banner", () => {
  const paused = ap({ state: "paused", reason: "Fallaron los tests" });

  it("shows the reason and resumes", async () => {
    const spy = vi.spyOn(api, "setAutopilot").mockResolvedValueOnce({} as never);
    const onDone = vi.fn();
    render(<HandoffActions task="A-1" handoff={null} autopilot={paused} onDone={onDone} />);
    expect(screen.getByText("Fallaron los tests")).not.toBeNull();
    fireEvent.click(screen.getByText("Reanudar autopiloto"));
    await waitFor(() => expect(spy).toHaveBeenCalledWith("A-1", "resume"));
    await waitFor(() => expect(onDone).toHaveBeenCalled());
  });

  it("turns autopilot off with Seguir a mano", async () => {
    const spy = vi.spyOn(api, "setAutopilot").mockResolvedValueOnce({} as never);
    render(<HandoffActions task="A-1" handoff={null} autopilot={paused} />);
    fireEvent.click(screen.getByText("Seguir a mano"));
    await waitFor(() => expect(spy).toHaveBeenCalledWith("A-1", "off"));
  });

  it("shows no banner when running", () => {
    render(<HandoffActions task="A-1" handoff={null} autopilot={ap({})} />);
    expect(screen.queryByText("Reanudar autopiloto")).toBeNull();
  });
});
