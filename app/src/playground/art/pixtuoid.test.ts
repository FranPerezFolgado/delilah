import { describe, expect, it } from "vitest";
import { ANIMATIONS, PALETTE, RAMPS, SPRITES } from "./pixtuoid";

describe("pixtuoid art", () => {
  for (const [name, s] of Object.entries(SPRITES)) {
    it(`sprite ${name} is well formed`, () => {
      expect(s.frames.length).toBeGreaterThan(0);
      let tallest = 0;
      for (const rows of s.frames) {
        tallest = Math.max(tallest, rows.length);
        for (const row of rows) {
          expect([...row]).toHaveLength(s.w);
          for (const ch of row) expect(ch === "." || ch in PALETTE || ch in RAMPS).toBe(true);
        }
      }
      expect(tallest).toBe(s.h);
      for (const [x, y] of Object.values(s.marks)) {
        expect(x).toBeGreaterThanOrEqual(0);
        expect(x).toBeLessThan(s.w);
        expect(y).toBeGreaterThanOrEqual(0);
        expect(y).toBeLessThan(s.h);
      }
    });
  }

  it("animations reference existing sprites", () => {
    for (const a of Object.values(ANIMATIONS)) for (const f of a.frames) expect(SPRITES[f]).toBeDefined();
  });
});
