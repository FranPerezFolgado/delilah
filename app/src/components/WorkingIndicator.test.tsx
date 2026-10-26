// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render } from "@testing-library/react";
import { setLanguage } from "../i18n";
import type { TurnProgress } from "../api";
import { WorkingIndicator } from "./WorkingIndicator";

const T0 = Date.parse("2026-01-01T00:00:00Z");
const turn = (over: Partial<TurnProgress> = {}): TurnProgress => ({
  started_at: "2026-01-01T00:00:00Z",
  by: "user",
  reason: null,
  step: null,
  last_signal_at: "2026-01-01T00:00:00Z",
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

describe("WorkingIndicator", () => {
  it("renders nothing without a turn", () => {
    const { container } = render(<WorkingIndicator turn={null} />);
    expect(container.firstChild).toBeNull();
  });

  it("shows a clock from started_at that advances", async () => {
    const { container } = render(<WorkingIndicator turn={turn()} />);
    expect(container.textContent).toContain("5 s");
    await act(() => vi.advanceTimersByTimeAsync(3000));
    expect(container.textContent).toContain("8 s");
    await act(() => vi.advanceTimersByTimeAsync(60000));
    expect(container.textContent).toContain("1 min 08 s");
  });

  it("shows the step", () => {
    const { container } = render(<WorkingIndicator turn={turn({ step: "Bash: cargo test" })} />);
    expect(container.textContent).toContain("Bash: cargo test");
  });

  it("shows no-news only from 30 s of silence, live", async () => {
    vi.setSystemTime(T0 + 29000);
    const { container } = render(<WorkingIndicator turn={turn()} />);
    expect(container.textContent).not.toContain("sin novedades");
    await act(() => vi.advanceTimersByTimeAsync(1000));
    expect(container.textContent).toContain("sin novedades desde hace 30 s");
    await act(() => vi.advanceTimersByTimeAsync(5000));
    expect(container.textContent).toContain("sin novedades desde hace 35 s");
  });

  it("drops no-news when a new signal arrives", () => {
    vi.setSystemTime(T0 + 40000);
    const { container, rerender } = render(<WorkingIndicator turn={turn()} />);
    expect(container.textContent).toContain("sin novedades");
    rerender(<WorkingIndicator turn={turn({ last_signal_at: "2026-01-01T00:00:39Z" })} />);
    expect(container.textContent).not.toContain("sin novedades");
    expect(container.textContent).toContain("40 s");
  });

  it("speaks English through t()", async () => {
    setLanguage("en");
    vi.setSystemTime(T0 + 40000);
    const { container } = render(<WorkingIndicator turn={turn()} />);
    expect(container.textContent).toContain("no news for 40 s");
    expect(container.textContent).toContain("Working");
  });
});
