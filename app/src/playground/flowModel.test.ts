import { describe, expect, it } from "vitest";
import type { Action, HistoryEntry, HistoryResponse } from "../api";
import { buildFlow } from "./flowModel";

const act = (id: string, tool = "Read", target: string | null = "a.ts"): Action => ({
  id, kind: "read", tool, target, background: false, since: "2026-01-01T00:00:01Z",
});
const at = (s: number) => `2026-01-01T00:00:${String(s).padStart(2, "0")}Z`;
const hist = (entries: HistoryEntry[], live = true, dropped = false): HistoryResponse => ({
  started_at: at(0),
  sessions: [{ session: "x", live, dropped, entries }],
});
const ts = (seq: number, a: Action, agent: { id: string; role: string } | null = null): HistoryEntry =>
  ({ seq, at: at(seq), type: "tool_started", agent, action: a });
const te = (seq: number, id: string, failed = false, agent = null): HistoryEntry =>
  ({ seq, at: at(seq), type: "tool_ended", agent, id, failed });

describe("buildFlow", () => {
  it("pairs start and end by tool use id and links agent to tool", () => {
    const m = buildFlow(hist([ts(1, act("a")), te(2, "a")]));
    const t = m.nodes.find((n) => n.id === "t:x:a")!;
    expect(t.state).toBe("done");
    expect(t.end).toBe(at(2));
    expect(m.edges).toContainEqual({ from: "s:x", to: "t:x:a" });
    expect(m.timeline.map((r) => r.event)).toEqual(["tool_started", "tool_ended"]);
  });

  it("makes parallel calls separate nodes", () => {
    const m = buildFlow(hist([ts(1, act("a")), ts(2, act("b")), te(3, "a")]));
    expect(m.nodes.filter((n) => n.type === "tool")).toHaveLength(2);
    expect(m.nodes.find((n) => n.id === "t:x:b")!.state).toBe("running");
  });

  it("attaches a helper's tools to the helper, helper to the lead", () => {
    const h = { id: "h1", role: "explorer" };
    const m = buildFlow(hist([ts(1, act("a"), h), { seq: 2, at: at(2), type: "helper_started", agent: h }]));
    expect(m.edges).toContainEqual({ from: "s:x", to: "h:x:h1" });
    expect(m.edges).toContainEqual({ from: "h:x:h1", to: "t:x:a" });
    expect(m.nodes.find((n) => n.id === "h:x:h1")!.label).toBe("explorer");
    expect(m.edges.filter((e) => e.to === "h:x:h1")).toHaveLength(1);
  });

  it("ends a helper", () => {
    const h = { id: "h1", role: "" };
    const m = buildFlow(hist([
      { seq: 1, at: at(1), type: "helper_started", agent: h },
      { seq: 2, at: at(2), type: "helper_ended", id: "h1" },
    ]));
    expect(m.nodes.find((n) => n.id === "h:x:h1")!.state).toBe("done");
  });

  it("ignores an end without a start", () => {
    const m = buildFlow(hist([te(1, "zzz"), { seq: 2, at: at(2), type: "helper_ended", id: "q" }]));
    expect(m.nodes.filter((n) => n.type !== "agent")).toHaveLength(0);
    expect(m.timeline).toHaveLength(0);
  });

  it("marks running calls unknown once the session ended", () => {
    const m = buildFlow(hist([ts(1, act("a"))], false));
    expect(m.nodes.find((n) => n.id === "t:x:a")!.state).toBe("unknown");
    expect(buildFlow(hist([ts(1, act("a"))], true)).nodes.find((n) => n.id === "t:x:a")!.state).toBe("running");
  });

  it("marks failed calls", () => {
    const m = buildFlow(hist([ts(1, act("a")), te(2, "a", true)]));
    expect(m.nodes.find((n) => n.id === "t:x:a")!.state).toBe("failed");
    expect(m.timeline[1].failed).toBe(true);
  });

  it("notes dropped and empty history", () => {
    expect(buildFlow(hist([], true, true)).notes).toEqual({ dropped: true, empty: true, startedAt: at(0) });
    expect(buildFlow(hist([ts(1, act("a"))])).notes.empty).toBe(false);
  });

  it("gives each session its own root and a turn_over row", () => {
    const m = buildFlow({
      started_at: at(0),
      sessions: [
        { session: "x", live: true, dropped: false, entries: [{ seq: 1, at: at(1), type: "turn_over" }] },
        { session: "y", live: true, dropped: false, entries: [ts(2, act("a"))] },
      ],
    });
    expect(m.nodes.filter((n) => n.type === "agent").map((n) => n.id)).toEqual(["s:x", "s:y"]);
    expect(m.timeline[0].node).toBe("s:x");
  });
});
