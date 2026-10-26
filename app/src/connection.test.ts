// @vitest-environment jsdom
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const h = vi.hoisted(() => ({ observers: new Set<(ok: boolean, at: number) => void>(), health: vi.fn() }));
vi.mock("./api", () => ({
  onRequestResult: (cb: (ok: boolean, at: number) => void) => {
    h.observers.add(cb);
    return () => void h.observers.delete(cb);
  },
  api: { health: h.health },
}));
import { useConnection } from "./connection";

const emit = (ok: boolean) => act(() => h.observers.forEach((cb) => cb(ok, Date.now())));
const tick = (ms: number) => act(() => vi.advanceTimersByTimeAsync(ms));

beforeEach(() => {
  vi.useFakeTimers();
  h.observers.clear();
  h.health.mockReset();
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("useConnection", () => {
  it("starts up, goes down on a failed request and up on the next success", () => {
    const { result } = renderHook(() => useConnection());
    expect(result.current.status).toBe("up");
    h.health.mockRejectedValue(new Error("x"));
    emit(false);
    expect(result.current.status).toBe("down");
    emit(true);
    expect(result.current.status).toBe("up");
    expect(result.current.lastOk).toBeGreaterThan(0);
  });

  it("polls health every 2 s, backing off to at most 5 s", async () => {
    h.health.mockRejectedValue(new Error("x"));
    renderHook(() => useConnection());
    emit(false);
    await tick(1999);
    expect(h.health).toHaveBeenCalledTimes(0);
    await tick(1);
    expect(h.health).toHaveBeenCalledTimes(1);
    await tick(60_000);
    const before = h.health.mock.calls.length;
    await tick(5000);
    expect(h.health.mock.calls.length - before).toBeGreaterThanOrEqual(1);
    expect(h.health.mock.calls.length - before).toBeLessThanOrEqual(2 + 0);
    // never waits longer than 5 s
    await tick(5000);
    expect(h.health.mock.calls.length - before).toBeGreaterThanOrEqual(2);
  });

  it("recovers on its own when health answers, and stops polling", async () => {
    h.health.mockRejectedValueOnce(new Error("x")).mockResolvedValue({});
    const { result } = renderHook(() => useConnection());
    emit(false);
    await tick(2000);
    expect(result.current.status).toBe("down");
    await tick(5000);
    expect(result.current.status).toBe("up");
    const n = h.health.mock.calls.length;
    await tick(20_000);
    expect(h.health.mock.calls.length).toBe(n);
  });

  it("retry() checks right away", async () => {
    h.health.mockResolvedValue({});
    const { result } = renderHook(() => useConnection());
    emit(false);
    await act(async () => result.current.retry());
    expect(h.health).toHaveBeenCalledTimes(1);
    expect(result.current.status).toBe("up");
  });
});
