import { describe, it, expect } from "vitest";
import { PALETTE, RAMPS } from "./pixtuoid";
import { RAMP_FIXTURES } from "./ramp.fixtures";
import { ramp, recolour } from "./ramp";

const chan = (h: string) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16));

describe("ramp", () => {
  it("level 0 is identity", () => {
    expect(ramp("#2e62cf", 0)).toBe("#2e62cf");
  });
  for (const [key, f] of Object.entries(RAMP_FIXTURES)) {
    it(`matches fixture ${key} (${f.of} ${f.level})`, () => {
      const got = chan(ramp(f.base, f.level));
      const want = chan(f.shade);
      got.forEach((c, i) => expect(c).toBe(want[i]));
    });
  }
});

describe("recolour", () => {
  it("sets bases and recomputes derived keys", () => {
    const base = { B: "#112233", P: "#445566", H: "#778899", S: "#aabbcc" };
    const out = recolour(base);
    expect(out.B).toBe("#112233");
    expect(out.S).toBe("#aabbcc");
    for (const [key, def] of Object.entries(RAMPS)) {
      if (def.of in base) expect(out[key]).toBe(ramp(base[def.of as keyof typeof base], def.level));
      else expect(out[key]).toBeUndefined();
    }
    expect(Object.keys(out).every((k) => k in PALETTE || k in RAMPS)).toBe(true);
  });
});
