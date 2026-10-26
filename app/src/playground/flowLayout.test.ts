import { describe, expect, it } from "vitest";
import type { HistoryEntry, HistoryResponse } from "../api";
import { buildFlow } from "./flowModel";
import { layoutFlow, NODE_H } from "./flowLayout";

const at = (s: number) => `2026-01-01T00:00:${String(s).padStart(2, "0")}Z`;
const tool = (seq: number, id: string, agent: { id: string; role: string } | null = null): HistoryEntry => ({
  seq, at: at(seq), type: "tool_started", agent,
  action: { id, kind: "run", tool: "Bash", target: null, background: false, since: at(seq) },
});
const hist = (entries: HistoryEntry[]): HistoryResponse => ({
  started_at: at(0),
  sessions: [{ session: "x", live: true, dropped: false, entries }],
});

describe("layoutFlow", () => {
  it("puts tools below their agent, newest at the bottom, helpers to the right", () => {
    const l = layoutFlow(buildFlow(hist([tool(1, "a"), tool(2, "b"), tool(3, "c", { id: "h", role: "r" })])));
    const p = l.positions;
    expect(p["t:x:b"].y).toBeGreaterThan(p["t:x:a"].y);
    expect(p["t:x:a"].y).toBeGreaterThanOrEqual(p["s:x"].y + NODE_H);
    expect(p["h:x:h"].x).toBeGreaterThan(p["s:x"].x);
    expect(p["t:x:c"].x).toBe(p["h:x:h"].x);
    expect(l.bounds.w).toBeGreaterThanOrEqual(p["h:x:h"].x + p["h:x:h"].w);
  });

  it("keeps positions when later events arrive", () => {
    const first = [tool(1, "a"), tool(2, "b", { id: "h", role: "r" })];
    const before = layoutFlow(buildFlow(hist(first))).positions;
    const after = layoutFlow(buildFlow(hist([...first, tool(3, "c"), tool(4, "d", { id: "h2", role: "" }), tool(5, "e", { id: "h", role: "r" })]))).positions;
    for (const [id, box] of Object.entries(before)) expect(after[id]).toEqual(box);
  });

  it("keeps positions when a later session appears", () => {
    const one = hist([tool(1, "a")]);
    const two: HistoryResponse = {
      ...one,
      sessions: [...one.sessions, { session: "y", live: true, dropped: false, entries: [tool(5, "z")] }],
    };
    const before = layoutFlow(buildFlow(one)).positions;
    const after = layoutFlow(buildFlow(two)).positions;
    for (const [id, box] of Object.entries(before)) expect(after[id]).toEqual(box);
  });

  it("returns empty bounds for an empty model", () => {
    expect(layoutFlow(buildFlow({ started_at: at(0), sessions: [] })).bounds).toEqual({ x: 0, y: 0, w: 0, h: 0 });
  });
});
