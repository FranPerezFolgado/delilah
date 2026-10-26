// @vitest-environment jsdom
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const platform = vi.hoisted(() => ({
  canUpdate: true,
  appVersion: vi.fn(),
  checkForUpdate: vi.fn(),
  relaunch: vi.fn(),
  onCheckUpdatesMenu: vi.fn(),
}));
vi.mock("./platform", () => platform);
const api = vi.hoisted(() => ({ sessions: vi.fn(), preference: vi.fn() }));
vi.mock("./api", () => ({ api }));

import { setLanguage } from "./i18n";
import { useUpdates } from "./useUpdates";

const flush = () => act(async () => { await vi.advanceTimersByTimeAsync(0); });

beforeEach(() => {
  setLanguage("es");
  vi.useFakeTimers();
  platform.canUpdate = true;
  platform.appVersion.mockReset().mockResolvedValue("1.0.0");
  platform.checkForUpdate.mockReset().mockResolvedValue(null);
  platform.relaunch.mockReset().mockResolvedValue(undefined);
  platform.onCheckUpdatesMenu.mockReset().mockResolvedValue(() => {});
  api.sessions.mockReset().mockResolvedValue([]);
  api.preference.mockReset().mockResolvedValue(null);
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

  it("skips scheduled checks when auto-update is off, but a manual check still runs", async () => {
    api.preference.mockResolvedValue(false);
    const { result } = renderHook(() => useUpdates());
    await act(async () => { await vi.advanceTimersByTimeAsync(10_000); });
    await act(async () => { await vi.advanceTimersByTimeAsync(6 * 60 * 60 * 1000); });
    expect(api.preference).toHaveBeenCalledWith("auto-update");
    expect(platform.checkForUpdate).not.toHaveBeenCalled();
    await act(async () => { await result.current.check(true); });
    expect(platform.checkForUpdate).toHaveBeenCalledTimes(1);
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

  describe("with sessions", () => {
    const sess = (live: boolean, closed = false) => ({ live, closed });
    async function offered() {
      const install = vi.fn(async () => {});
      platform.checkForUpdate.mockResolvedValue({ version: "1.1.0", notes: "", install });
      const hook = renderHook(() => useUpdates());
      await act(async () => { await vi.advanceTimersByTimeAsync(10_000); });
      return { ...hook, install };
    }
    const request = async (r: { current: ReturnType<typeof useUpdates> }) => {
      await act(async () => { r.current.dispatch({ type: "requestUpdate" }); });
      await flush();
    };

    it("asks for confirmation with live sessions, counting only live and open ones", async () => {
      api.sessions.mockResolvedValue([sess(true), sess(true), sess(false), sess(true, true)]);
      const { result, install } = await offered();
      await request(result);
      expect(result.current.state).toMatchObject({ kind: "confirmSessions", running: 2 });
      expect(install).not.toHaveBeenCalled();
    });

    it("updates straight away with no live sessions", async () => {
      api.sessions.mockResolvedValue([sess(false), sess(true, true)]);
      const { result, install } = await offered();
      await request(result);
      expect(install).toHaveBeenCalledTimes(1);
    });

    it("'now' downloads and installs", async () => {
      api.sessions.mockResolvedValue([sess(true), sess(true)]);
      const { result, install } = await offered();
      await request(result);
      await act(async () => { result.current.dispatch({ type: "now" }); });
      await flush();
      expect(install).toHaveBeenCalledTimes(1);
    });

    it("cancel returns to available", async () => {
      api.sessions.mockResolvedValue([sess(true), sess(true)]);
      const { result } = await offered();
      await request(result);
      await act(async () => { result.current.dispatch({ type: "cancel" }); });
      expect(result.current.state.kind).toBe("available");
    });

    it("whenIdle waits, polls every 10 s and installs at zero", async () => {
      api.sessions.mockResolvedValue([sess(true), sess(true)]);
      const { result, install } = await offered();
      await request(result);
      await act(async () => { result.current.dispatch({ type: "whenIdle" }); });
      expect(result.current.state.kind).toBe("waiting");
      api.sessions.mockResolvedValue([sess(true)]);
      await act(async () => { await vi.advanceTimersByTimeAsync(10_000); });
      expect(result.current.state).toMatchObject({ kind: "waiting", running: 1 });
      expect(install).not.toHaveBeenCalled();
      api.sessions.mockResolvedValue([]);
      await act(async () => { await vi.advanceTimersByTimeAsync(10_000); });
      expect(install).toHaveBeenCalledTimes(1);
    });

    it("stops polling when leaving waiting", async () => {
      api.sessions.mockResolvedValue([sess(true)]);
      const { result } = await offered();
      await request(result);
      await act(async () => { result.current.dispatch({ type: "whenIdle" }); });
      await act(async () => { result.current.dispatch({ type: "cancel" }); });
      api.sessions.mockClear();
      await act(async () => { await vi.advanceTimersByTimeAsync(30_000); });
      expect(api.sessions).not.toHaveBeenCalled();
    });

    it("a failing api.sessions() keeps waiting and never installs", async () => {
      api.sessions.mockResolvedValue([sess(true)]);
      const { result, install } = await offered();
      await request(result);
      await act(async () => { result.current.dispatch({ type: "whenIdle" }); });
      api.sessions.mockRejectedValue(new Error("down"));
      await act(async () => { await vi.advanceTimersByTimeAsync(20_000); });
      expect(result.current.state.kind).toBe("waiting");
      expect(install).not.toHaveBeenCalled();
    });

    it("a failing api.sessions() on request never installs", async () => {
      api.sessions.mockRejectedValue(new Error("down"));
      const { result, install } = await offered();
      await request(result);
      expect(result.current.state).toMatchObject({ kind: "error", reason: "down", version: "1.1.0" });
      expect(install).not.toHaveBeenCalled();
    });

    it("retrying after that failure asks for the sessions again", async () => {
      api.sessions.mockRejectedValue(new Error("down"));
      const { result, install } = await offered();
      await request(result);
      api.sessions.mockResolvedValue([sess(true)]);
      await act(async () => { result.current.dispatch({ type: "retry" }); });
      await flush();
      expect(result.current.state).toMatchObject({ kind: "confirmSessions", running: 1 });
      expect(install).not.toHaveBeenCalled();
    });
  });

  describe("manual checks and the menu", () => {
    const menu = async () => {
      const hook = renderHook(() => useUpdates());
      await flush();
      const cb = platform.onCheckUpdatesMenu.mock.calls[0][0] as () => void;
      return { ...hook, cb };
    };

    it("the menu runs a manual check", async () => {
      const { result, cb } = await menu();
      await act(async () => { cb(); });
      await flush();
      expect(platform.checkForUpdate).toHaveBeenCalledTimes(1);
      expect(result.current.state).toEqual({ kind: "upToDate", manual: true });
    });

    it("the menu in a build without updates answers with an error", async () => {
      platform.canUpdate = false;
      const { result, cb } = await menu();
      await act(async () => { cb(); });
      expect(platform.checkForUpdate).not.toHaveBeenCalled();
      expect(result.current.state).toMatchObject({ kind: "error", reason: "Las actualizaciones solo están en la app instalada" });
    });

    it("unsubscribes from the menu on unmount", async () => {
      const unlisten = vi.fn();
      platform.onCheckUpdatesMenu.mockResolvedValue(unlisten);
      const { unmount } = await menu();
      unmount();
      await flush();
      expect(unlisten).toHaveBeenCalled();
    });

    it("retry after a failed manual check checks again", async () => {
      platform.checkForUpdate.mockRejectedValueOnce(new Error("red")).mockResolvedValue(null);
      const { result } = renderHook(() => useUpdates());
      await flush();
      await act(async () => { await result.current.check(true); });
      expect(result.current.state).toMatchObject({ kind: "error", reason: "red" });
      await act(async () => { result.current.dispatch({ type: "retry" }); });
      await flush();
      expect(platform.checkForUpdate).toHaveBeenCalledTimes(2);
      expect(result.current.state.kind).toBe("upToDate");
    });
  });
});
