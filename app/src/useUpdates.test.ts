// @vitest-environment jsdom
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const platform = vi.hoisted(() => ({
  canUpdate: true,
  appVersion: vi.fn(),
  checkForUpdate: vi.fn(),
  relaunch: vi.fn(),
}));
vi.mock("./platform", () => platform);

import { useUpdates } from "./useUpdates";

const flush = () => act(async () => { await vi.advanceTimersByTimeAsync(0); });

beforeEach(() => {
  vi.useFakeTimers();
  platform.canUpdate = true;
  platform.appVersion.mockReset().mockResolvedValue("1.0.0");
  platform.checkForUpdate.mockReset().mockResolvedValue(null);
  platform.relaunch.mockReset().mockResolvedValue(undefined);
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("useUpdates", () => {
  it("checks at 10 s and then every 6 h", async () => {
    const { result } = renderHook(() => useUpdates());
    await flush();
    expect(result.current.meta.installed).toBe("1.0.0");
    expect(platform.checkForUpdate).not.toHaveBeenCalled();
    await act(async () => { await vi.advanceTimersByTimeAsync(10_000); });
    expect(platform.checkForUpdate).toHaveBeenCalledTimes(1);
    await act(async () => { await vi.advanceTimersByTimeAsync(6 * 60 * 60 * 1000); });
    expect(platform.checkForUpdate).toHaveBeenCalledTimes(2);
  });

  it("reaches available when an update is found", async () => {
    platform.checkForUpdate.mockResolvedValue({ version: "1.1.0", notes: "n", install: vi.fn() });
    const { result } = renderHook(() => useUpdates());
    await act(async () => { await vi.advanceTimersByTimeAsync(10_000); });
    expect(result.current.state).toEqual({ kind: "available", version: "1.1.0", notes: "n" });
  });

  it("update installs with progress and then relaunches", async () => {
    const install = vi.fn(async (on: (r: number, t: number | null) => void) => { on(5, 10); });
    platform.checkForUpdate.mockResolvedValue({ version: "1.1.0", notes: "", install });
    const { result } = renderHook(() => useUpdates());
    await act(async () => { await vi.advanceTimersByTimeAsync(10_000); });
    await act(async () => { result.current.dispatch({ type: "update", running: 0 }); });
    await flush();
    expect(install).toHaveBeenCalledTimes(1);
    expect(platform.relaunch).toHaveBeenCalledTimes(1);
    expect(result.current.state.kind).toBe("installing");
  });

  it("a failed automatic check stays idle", async () => {
    platform.checkForUpdate.mockRejectedValue(new Error("red"));
    const { result } = renderHook(() => useUpdates());
    await act(async () => { await vi.advanceTimersByTimeAsync(10_000); });
    expect(result.current.state.kind).toBe("idle");
  });

  it("does nothing when updating is unavailable", async () => {
    platform.canUpdate = false;
    renderHook(() => useUpdates());
    await act(async () => { await vi.advanceTimersByTimeAsync(7 * 60 * 60 * 1000); });
    expect(platform.appVersion).not.toHaveBeenCalled();
    expect(platform.checkForUpdate).not.toHaveBeenCalled();
  });
});
