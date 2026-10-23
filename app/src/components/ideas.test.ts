import { describe, expect, it } from "vitest";
import type { Session } from "../api";
import { ideaToOpen } from "./ideas";

const idea = (id: string, state = "idle"): Session =>
  ({
    id,
    task: "@web",
    kind: "claude",
    label: "Idea",
    primary: false,
    closed: false,
    live: true,
    read_only: false,
    state,
    conversation: true,
    step: null,
  }) as Session;

describe("ideaToOpen", () => {
  it("returns the only idea", () => {
    const a = idea("a");
    expect(ideaToOpen([a], null)).toBe(a);
  });

  it("prefers the first waiting idea over the last shown", () => {
    const l = [idea("a"), idea("b", "waiting"), idea("c", "waiting")];
    expect(ideaToOpen(l, "a").id).toBe("b");
  });

  it("returns the last shown when none waits", () => {
    const l = [idea("a"), idea("b"), idea("c")];
    expect(ideaToOpen(l, "a").id).toBe("a");
  });

  it("falls back to the newest when last shown is not listed", () => {
    const l = [idea("a"), idea("b"), idea("c")];
    expect(ideaToOpen(l, "x").id).toBe("c");
  });

  it("falls back to the newest when nothing was shown", () => {
    const l = [idea("a"), idea("b"), idea("c")];
    expect(ideaToOpen(l, null).id).toBe("c");
  });
});
