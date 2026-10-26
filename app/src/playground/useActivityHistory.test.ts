// @vitest-environment jsdom
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { HistoryEntry, HistoryResponse } from "../api";

const history = vi.fn();
const handlers: (() => void)[] = [];
vi.mock("../api", () => ({ api: { activityHistory: (...a: unknown[]) => history(...a) } }));
vi.mock("../live", () => ({
  on: (_t: string, cb: () => void) => {
    handlers.push(cb);
    return () => handlers.splice(handlers.indexOf(cb), 1);
  },
  connect: () => () => {},
}));

import { useActivityHistory } from "./useActivityHistory";

const entry = (seq: number): HistoryEntry => ({ seq, at: "2026-01-01T00:00:00Z", type: "turn_over" });
const resp = (sessions: [string, number[], boolean?][]): HistoryResponse => ({
  started_at: "t0",
  sessions: sessions.map(([session, seqs, live]) => ({ session, live: live ?? true, dropped: false, entries: seqs.map(entry) })),
});
const flush = () => act(async () => { await Promise.resolve(); await Promise.resolve(); });
const fire = async () => { await act(async () => handlers.forEach((h) => h())); await flush(); };

afterEach(cleanup);
beforeEach(() => {
  history.mockReset();
  handlers.length = 0;
});

describe("useActivityHistory", () => {
  it("fetches everything on mount", async () => {
    history.mockResolvedValue(resp([["a", [1, 2]]]));
    const { result } = renderHook(() => useActivityHistory("me", true));
    await flush();
    expect(history).toHaveBeenCalledWith("me", undefined);
    expect(result.current.startedAt).toBe("t0");
    expect(result.current.sessions[0].entries.map((e) => e.seq)).toEqual([1, 2]);
    expect(result.current.stale).toBe(false);
  });

  it("asks after the last seq on a topic and appends per session", async () => {
    history.mockResolvedValueOnce(resp([["a", [1, 2]], ["b", [3]]]));
    const { result } = renderHook(() => useActivityHistory("me", true));
    await flush();
    history.mockResolvedValueOnce(resp([["a", [4, 2], false], ["c", [5]]]));
    await fire();
    expect(history).toHaveBeenLastCalledWith("me", 3);
    const by = Object.fromEntries(result.current.sessions.map((s) => [s.session, s]));
    expect(by.a.entries.map((e) => e.seq)).toEqual([1, 2, 4]);
    expect(by.a.live).toBe(false);
    expect(by.b.entries.map((e) => e.seq)).toEqual([3]);
    expect(by.c.entries.map((e) => e.seq)).toEqual([5]);
  });

  it("ignores a slower, older response", async () => {
    history.mockResolvedValueOnce(resp([["a", [1]]]));
    const { result } = renderHook(() => useActivityHistory("me", true));
    await flush();
    let slow!: (r: HistoryResponse) => void;
    history.mockReturnValueOnce(new Promise((r) => (slow = r)));
    await fire();
    history.mockResolvedValueOnce(resp([["a", [2, 3]]]));
    await fire();
    await act(async () => slow(resp([["a", [2]]])));
    expect(result.current.sessions[0].entries.map((e) => e.seq)).toEqual([1, 2, 3]);
  });

  it("is stale while offline and refetches everything on reconnect", async () => {
    history.mockResolvedValue(resp([["a", [1]]]));
    const { result, rerender } = renderHook(({ on }) => useActivityHistory("me", on), { initialProps: { on: true } });
    await flush();
    rerender({ on: false });
    expect(result.current.stale).toBe(true);
    expect(result.current.sessions).toHaveLength(1);
    history.mockResolvedValue(resp([["a", [1, 2]]]));
    rerender({ on: true });
    await flush();
    expect(history).toHaveBeenLastCalledWith("me", undefined);
    expect(result.current.stale).toBe(false);
    expect(result.current.sessions[0].entries).toHaveLength(2);
  });

  it("resets and refetches when the owner changes", async () => {
    history.mockResolvedValueOnce(resp([["a", [1]]]));
    const { result, rerender } = renderHook(({ o }) => useActivityHistory(o, true), { initialProps: { o: "one" } });
    await flush();
    history.mockResolvedValueOnce(resp([["z", [9]]]));
    rerender({ o: "two" });
    expect(result.current.sessions).toEqual([]);
    await flush();
    expect(history).toHaveBeenLastCalledWith("two", undefined);
    expect(result.current.sessions.map((s) => s.session)).toEqual(["z"]);
  });
});
