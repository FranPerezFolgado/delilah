import { describe, expect, it, vi } from "vitest";
import { handleSessionMessage, on } from "./live";

describe("per-session live messages", () => {
  it("delivers a progress message to subscribers of that session", () => {
    const fn = vi.fn();
    const other = vi.fn();
    const off = on("s1", "progress", fn);
    on("s2", "progress", other);
    const p = { turn: null, background: [], subagents: [] };
    handleSessionMessage("s1", JSON.stringify({ progress: p }));
    expect(fn).toHaveBeenCalledWith(p);
    expect(other).not.toHaveBeenCalled();
    off();
    handleSessionMessage("s1", JSON.stringify({ progress: p }));
    expect(fn).toHaveBeenCalledTimes(1);
  });

  it("ignores text, done, reset and garbage", () => {
    const fn = vi.fn();
    on("s3", "progress", fn);
    for (const m of [{ message: "m", text: "x" }, { message: "m", done: true }, { reset: true }]) {
      handleSessionMessage("s3", JSON.stringify(m));
    }
    handleSessionMessage("s3", "not json");
    expect(fn).not.toHaveBeenCalled();
  });
});
