// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { onOpenTask, openPlaygroundWindow, requestOpenTask } from "./platform";

// A tiny in-memory BroadcastChannel: delivers to the other instances of the same name.
const channels = new Map<string, Set<FakeChannel>>();
class FakeChannel {
  onmessage: ((e: { data: unknown }) => void) | null = null;
  private listeners = new Set<(e: { data: unknown }) => void>();
  constructor(public name: string) {
    if (!channels.has(name)) channels.set(name, new Set());
    channels.get(name)!.add(this);
  }
  postMessage(data: unknown) {
    for (const c of channels.get(this.name) ?? []) {
      if (c === this) continue;
      c.onmessage?.({ data });
      c.listeners.forEach((l) => l({ data }));
    }
  }
  addEventListener(_: string, l: (e: { data: unknown }) => void) {
    this.listeners.add(l);
  }
  removeEventListener(_: string, l: (e: { data: unknown }) => void) {
    this.listeners.delete(l);
  }
  close() {
    channels.get(this.name)?.delete(this);
  }
}

beforeEach(() => {
  channels.clear();
  vi.stubGlobal("BroadcastChannel", FakeChannel);
});
afterEach(() => vi.unstubAllGlobals());

describe("platform (browser mode)", () => {
  it("opens the playground in a named tab", async () => {
    const open = vi.fn().mockReturnValue({ focus: vi.fn() });
    vi.stubGlobal("open", open);
    await openPlaygroundWindow();
    expect(open).toHaveBeenCalledTimes(1);
    expect(open.mock.calls[0][0]).toMatch(/#playground$/);
    expect(open.mock.calls[0][1]).toBe("delilah-playground");
  });

  it("requestOpenTask posts an open-task message on the delilah channel", async () => {
    const got: unknown[] = [];
    const rx = new FakeChannel("delilah");
    rx.onmessage = (e) => got.push(e.data);
    await requestOpenTask({ task: "W-1", session: "s1" });
    expect(got).toEqual([{ type: "open-task", task: "W-1", session: "s1" }]);
  });

  it("onOpenTask calls back for that message and stops after unsubscribe", async () => {
    const cb = vi.fn();
    const off = await onOpenTask(cb);
    await requestOpenTask({ task: "W-1", session: "s1" });
    expect(cb).toHaveBeenCalledWith({ task: "W-1", session: "s1" });
    off();
    await requestOpenTask({ task: "W-2", session: "s2" });
    expect(cb).toHaveBeenCalledTimes(1);
  });
});
