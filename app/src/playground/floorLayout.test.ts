import { describe, expect, it } from "vitest";
import {
  AISLE_X, AISLE_Y, DESK_W, LEAVER_MS, POD_COLS, POD_H, POD_W, WALL_H, assignSlots, deskPosition, floorCols, floorSize,
  floorOrder, loungeLeft, loungePath, loungeSeats, pathOffsets, podCount, seatPosition, sidePosition, trackLeavers,
} from "./floorLayout";

describe("assignSlots", () => {
  it("does not hand a reserved slot (a leaver still walking from it) to a new id", () => {
    const m = assignSlots(new Map(), ["l1"], ["h3"], [1]);
    expect(m.get("l1")).toBe(0);
    expect(m.get("h3")).toBe(2);
  });

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

  it("the seat is centred on its desk, with the head above the monitor and the body behind the desk", () => {
    const d = deskPosition(3);
    const s = seatPosition(3);
    expect(s.x).toBe(d.x + DESK_W / 2);
    // The sprite's top (feet - 12) is above the desk's top edge; its feet are behind the desk.
    expect(s.y - 12).toBeLessThan(d.y);
    expect(s.y).toBeGreaterThan(d.y);
  });

  it("the first desk row starts below the wall, with room for the head", () => {
    expect(deskPosition(0).y - 5).toBeGreaterThanOrEqual(WALL_H);
  });

  it("a sitter's head clears the desk row above it", () => {
    expect(seatPosition(2).y - 12).toBeGreaterThan(deskPosition(0).y + 9);
  });

  it("desks fit in the floor", () => {
    const size = floorSize(podCount(4 * POD_COLS + 1));
    const s = seatPosition(4 * POD_COLS);
    expect(s.y).toBeLessThan(size.height);
  });

  it("the floor is sized to its content, not a tall box", () => {
    const one = floorSize(1);
    const two = floorSize(2);
    expect(two).toEqual(one);
    expect(two.height).toBeLessThan(80);
    expect(floorCols(1)).toBe(2);
    expect(floorCols(9)).toBe(POD_COLS);
  });

  it("the lounge sits right of the pods and inside the floor", () => {
    for (const pods of [1, 2, 3, 5]) {
      const size = floorSize(pods);
      expect(loungeLeft(pods)).toBeGreaterThan(deskPosition(4 * (floorCols(pods) - 1)).x + POD_W - 1);
      for (const seat of loungeSeats(pods)) {
        expect(seat.x).toBeLessThan(size.width);
        expect(seat.y).toBeLessThan(size.height);
      }
    }
  });
});

describe("loungePath", () => {
  it("goes seat, beside the desk, down to the aisle, along it to the lounge, then a lounge seat", () => {
    const path = loungePath(1, 2);
    expect(path).toHaveLength(5);
    expect(path[0]).toEqual(seatPosition(1));
    expect(path[1]).toEqual(sidePosition(1));
    expect(path[2].x).toBe(path[1].x);
    expect(path[2].y).toBeGreaterThan(path[1].y);
    expect(path[3].y).toBe(path[2].y);
    expect(loungeSeats(2)).toContainEqual(path[4]);
    expect(path[4].x).toBeGreaterThan(path[0].x);
  });

  it("the walker steps out toward the middle of its pod row", () => {
    expect(sidePosition(0).x).toBeGreaterThan(seatPosition(0).x);
    expect(sidePosition(1).x).toBeLessThan(seatPosition(1).x);
  });

  it("spreads slots over several lounge seats", () => {
    const ends = new Set([0, 1, 2, 3].map((s) => JSON.stringify(loungePath(s, 1).at(-1))));
    expect(ends.size).toBeGreaterThan(1);
  });

  it("offsets run from 0 to 1 by length", () => {
    const o = pathOffsets([{ x: 0, y: 0 }, { x: 10, y: 0 }, { x: 10, y: 30 }]);
    expect(o).toEqual([0, 0.25, 1]);
  });
});

describe("trackLeavers", () => {
  const seat = (id: string, slot: number) => ({ id, slot });

  it("a helper that is gone becomes a leaver from its slot, keeping its role", () => {
    const out = trackLeavers({ helpers: [{ ...seat("h1", 2), role: "Explore" }, seat("h2", 3)], leaving: [] }, ["h2"], 1000);
    expect(out).toEqual([{ id: "h1", slot: 2, role: "Explore", since: 1000 }]);
  });

  it("a leaver stays ~8.5 s: ~3 s walking, ~5 s in the lounge, a fade", () => {
    expect(LEAVER_MS).toBe(8500);
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
