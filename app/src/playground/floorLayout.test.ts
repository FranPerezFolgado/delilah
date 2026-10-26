import { describe, expect, it } from "vitest";
import {
  AISLE_X, AISLE_Y, DESK_H, DESK_W, LEAVER_MS, LOUNGE_SEATS, POD_COLS, POD_H, POD_W, assignSlots, deskPosition, floorSize,
  floorOrder, loungePath, podCount, seatPosition, trackLeavers,
} from "./floorLayout";

describe("assignSlots", () => {
  it("puts leads first, then helpers", () => {
    const m = assignSlots(new Map(), ["l1", "l2"], ["h1"]);
    expect([...m]).toEqual([["l1", 0], ["l2", 1], ["h1", 2]]);
  });

  it("keeps slots stable across updates", () => {
    const prev = assignSlots(new Map(), ["l1"], ["h1", "h2"]);
    const next = assignSlots(prev, ["l1"], ["h2", "h1"]);
    expect(next.get("l1")).toBe(0);
    expect(next.get("h1")).toBe(prev.get("h1"));
    expect(next.get("h2")).toBe(prev.get("h2"));
  });

  it("reuses a freed slot for the next newcomer", () => {
    const prev = assignSlots(new Map(), ["l1"], ["h1", "h2"]);
    const next = assignSlots(prev, ["l1"], ["h2", "h3"]);
    expect(next.has("h1")).toBe(false);
    expect(next.get("h3")).toBe(prev.get("h1"));
    expect(next.get("h2")).toBe(prev.get("h2"));
  });

  it("a new lead takes the lowest free slot, even if helpers already sit there later", () => {
    const prev = assignSlots(new Map(), [], ["h1"]);
    const next = assignSlots(prev, ["l1"], ["h1"]);
    expect(next.get("h1")).toBe(0);
    expect(next.get("l1")).toBe(1);
  });
});

describe("geometry", () => {
  it("grows by pods: more than 4 desks needs a second pod", () => {
    expect(podCount(0)).toBe(1);
    expect(podCount(4)).toBe(1);
    expect(podCount(5)).toBe(2);
    const a = floorSize(1);
    const b = floorSize(POD_COLS + 1);
    expect(b.width).toBeGreaterThanOrEqual(a.width);
    expect(b.height).toBeGreaterThan(a.height);
  });

  it("slot n sits in pod floor(n/4), desk n%4; pods fill left to right then down", () => {
    const d0 = deskPosition(0);
    const d1 = deskPosition(1);
    const d2 = deskPosition(2);
    expect(d1.x).toBeGreaterThan(d0.x);
    expect(d1.y).toBe(d0.y);
    expect(d2.x).toBe(d0.x);
    expect(d2.y).toBeGreaterThan(d0.y);
    const p1 = deskPosition(4);
    expect(p1.x).toBe(d0.x + POD_W + AISLE_X);
    expect(p1.y).toBe(d0.y);
    const row2 = deskPosition(4 * POD_COLS);
    expect(row2.x).toBe(d0.x);
    expect(row2.y).toBe(d0.y + POD_H + AISLE_Y);
  });

  it("the seat is centred below its desk", () => {
    const d = deskPosition(3);
    const s = seatPosition(3);
    expect(s.x).toBe(d.x + DESK_W / 2);
    expect(s.y).toBeGreaterThan(d.y + DESK_H - 1);
  });

  it("desks fit in the floor", () => {
    const size = floorSize(podCount(4 * POD_COLS + 1));
    const s = seatPosition(4 * POD_COLS);
    expect(s.y).toBeLessThan(size.height);
  });
});

describe("loungePath", () => {
  it("goes seat, aisle below the pod, lounge seat", () => {
    const path = loungePath(1);
    expect(path).toHaveLength(3);
    expect(path[0]).toEqual(seatPosition(1));
    expect(path[1].x).toBe(path[0].x);
    expect(path[1].y).toBeGreaterThan(path[0].y);
    expect(LOUNGE_SEATS).toContainEqual(path[2]);
    expect(path[2].x).toBeGreaterThan(path[0].x);
  });

  it("spreads slots over several lounge seats", () => {
    const ends = new Set([0, 1, 2, 3].map((s) => JSON.stringify(loungePath(s)[2])));
    expect(ends.size).toBeGreaterThan(1);
  });
});

describe("trackLeavers", () => {
  const seat = (id: string, slot: number) => ({ id, slot });

  it("a helper that is gone becomes a leaver for 5 s from its slot", () => {
    const out = trackLeavers({ helpers: [seat("h1", 2), seat("h2", 3)], leaving: [] }, ["h2"], 1000);
    expect(out).toEqual([{ id: "h1", slot: 2, since: 1000 }]);
  });

  it("keeps a leaver until it expires", () => {
    const prev = { helpers: [], leaving: [{ id: "h1", slot: 2, since: 1000 }] };
    expect(trackLeavers(prev, [], 1000 + LEAVER_MS - 1)).toHaveLength(1);
    expect(trackLeavers(prev, [], 1000 + LEAVER_MS)).toEqual([]);
  });

  it("does not duplicate a leaver and ignores one that came back", () => {
    const prev = { helpers: [], leaving: [{ id: "h1", slot: 2, since: 1000 }] };
    expect(trackLeavers(prev, ["h1"], 2000)).toEqual([]);
  });
});

describe("floorOrder", () => {
  it("puts new owners on top, newest first, and keeps the others in place", () => {
    expect(floorOrder([], ["a", "b"])).toEqual(["b", "a"]);
    expect(floorOrder(["b", "a"], ["a", "b", "c"])).toEqual(["c", "b", "a"]);
  });
  it("does not reorder when the input order changes", () => {
    expect(floorOrder(["c", "b", "a"], ["a", "c", "b"])).toEqual(["c", "b", "a"]);
  });
  it("drops the owners that are gone", () => {
    expect(floorOrder(["c", "b", "a"], ["a", "c"])).toEqual(["c", "a"]);
  });
});
