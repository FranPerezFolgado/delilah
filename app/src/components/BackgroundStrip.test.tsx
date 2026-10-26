// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render } from "@testing-library/react";
import { setLanguage } from "../i18n";
import type { BackgroundItem } from "../api";
import { BackgroundStrip } from "./BackgroundStrip";

const T0 = Date.parse("2026-01-01T00:00:00Z");
const item = (over: Partial<BackgroundItem> = {}): BackgroundItem => ({
  task_id: "t1",
  tool_use_id: null,
  kind: "command",
  description: "cargo watch",
  state: "running",
  started_at: "2026-01-01T00:00:00Z",
  ended_at: null,
  ...over,
});

beforeEach(() => {
  setLanguage("es");
  vi.useFakeTimers();
  vi.setSystemTime(T0 + 5000);
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("BackgroundStrip", () => {
  it("is hidden with no items", () => {
    const { container } = render(<BackgroundStrip items={[]} />);
    expect(container.firstChild).toBeNull();
  });

  it("renders a titled row per item with its description", () => {
    const { container } = render(
      <BackgroundStrip items={[item(), item({ task_id: "t2", kind: "agent", description: "revisar" })]} />,
    );
    expect(container.textContent).toContain("En segundo plano");
    expect(container.querySelectorAll(".bg-row")).toHaveLength(2);
    expect(container.textContent).toContain("cargo watch");
    expect(container.textContent).toContain("revisar");
  });

  it("ticks a clock while running", async () => {
    const { container } = render(<BackgroundStrip items={[item()]} />);
    expect(container.textContent).toContain("en curso");
    expect(container.textContent).toContain("desde hace 5 s");
    await act(() => vi.advanceTimersByTimeAsync(3000));
    expect(container.textContent).toContain("desde hace 8 s");
  });

  it("shows total time once ended and does not tick", async () => {
    const { container } = render(
      <BackgroundStrip items={[item({ state: "completed", ended_at: "2026-01-01T00:01:05.5Z" })]} />,
    );
    expect(container.textContent).toContain("terminado");
    expect(container.textContent).toContain("1 min 05 s");
    expect(container.textContent).not.toContain("desde hace");
    await act(() => vi.advanceTimersByTimeAsync(10000));
    expect(container.textContent).toContain("1 min 05 s");
  });

  it("labels every state", () => {
    const states: [BackgroundItem["state"], string][] = [
      ["failed", "fallido"],
      ["stopped", "detenido"],
      ["unknown", "estado desconocido"],
    ];
    for (const [state, label] of states) {
      const { container, unmount } = render(
        <BackgroundStrip items={[item({ state, ended_at: "2026-01-01T00:00:02Z" })]} />,
      );
      expect(container.textContent).toContain(label);
      unmount();
    }
    const { container } = render(<BackgroundStrip items={[item({ state: "weird" as never })]} />);
    expect(container.textContent).toContain("estado desconocido");
  });

  it("shows 'desde hace ?' without started_at", () => {
    const { container } = render(<BackgroundStrip items={[item({ started_at: null })]} />);
    expect(container.textContent).toContain("desde hace ?");
  });

  it("speaks English through t()", () => {
    setLanguage("en");
    const { container } = render(<BackgroundStrip items={[item({ started_at: null })]} />);
    expect(container.textContent).toContain("In the background");
    expect(container.textContent).toContain("for ?");
  });
});
