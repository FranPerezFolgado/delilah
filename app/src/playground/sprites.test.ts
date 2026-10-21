import { describe, expect, it } from "vitest";
import { DESK, PALETTE_KEYS, POSES } from "./sprites";

const NEW = ["typing", "reading", "terminal", "phone", "busy"];

describe("sprites", () => {
  it("has the action poses", () => {
    for (const p of NEW) expect(Object.keys(POSES)).toContain(p);
  });

  it("every pose is 2 frames of 16 rows x 16 chars with known palette keys", () => {
    for (const [name, frames] of Object.entries(POSES)) {
      expect(frames, name).toHaveLength(2);
      for (const rows of frames) {
        expect(rows, name).toHaveLength(16);
        for (const row of rows) {
          expect(row.length, name).toBe(16);
          for (const ch of row) if (ch !== ".") expect(PALETTE_KEYS, name).toContain(ch);
        }
      }
    }
  });

  it("the desk only uses palette keys", () => {
    for (const ch of DESK.flat().join("")) if (ch !== ".") expect(PALETTE_KEYS).toContain(ch);
  });
});
