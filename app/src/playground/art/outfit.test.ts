import { describe, it, expect } from "vitest";
import { OUTFITS, HAIR_PRESETS, SKIN_PRESETS, stringSeed, lookFor, roleLook } from "./outfit";

describe("outfit", () => {
  it("has the preset sizes", () => {
    expect(OUTFITS).toHaveLength(16);
    expect(HAIR_PRESETS).toHaveLength(8);
    expect(SKIN_PRESETS).toHaveLength(5);
  });
  it("golden seeds", () => {
    expect(stringSeed("delilah") % 16n).toBe(5n);
    expect(stringSeed("delilah")).toBe(16212437727587249893n);
    expect(stringSeed("/Users/x/proj") % 16n).toBe(15n);
    expect(stringSeed("/Users/x/proj")).toBe(5678970752872289743n);
  });
  it("lookFor is deterministic and keeps the outfit per project", () => {
    expect(lookFor("delilah", "s1")).toEqual(lookFor("delilah", "s1"));
    const a = lookFor("delilah", "s1");
    const b = lookFor("delilah", "s2");
    expect(a.B).toBe(b.B);
    expect(a.P).toBe(b.P);
    expect(a.B).toBe(OUTFITS[5].B);
  });
  it("roleLook is stable per role and differs between roles", () => {
    expect(roleLook("Explore")).toEqual(roleLook("Explore"));
    const roles = ["Explore", "code-researcher", "Plan", "general-purpose", "implementer"];
    const looks = new Set(roles.map((r) => JSON.stringify(roleLook(r))));
    expect(looks.size).toBe(roles.length);
    expect(roleLook("Explore")).not.toEqual(roleLook("Plan"));
    expect(OUTFITS.map((o) => o.B)).toContain(roleLook("Plan").B);
  });
});
