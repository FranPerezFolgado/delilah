import { describe, it, expect } from "vitest";
import { duration, formatTokens } from "./format";

describe("duration", () => {
  it("formats seconds and minutes", () => {
    expect(duration(0)).toBe("0 s");
    expect(duration(59)).toBe("59 s");
    expect(duration(60)).toBe("1 min 00 s");
    expect(duration(185)).toBe("3 min 05 s");
  });
  it("clamps negatives to zero", () => {
    expect(duration(-5)).toBe("0 s");
  });
});

describe("formatTokens", () => {
  it("formats magnitudes", () => {
    expect(formatTokens(950)).toBe("950");
    expect(formatTokens(1000)).toBe("1K");
    expect(formatTokens(12_300)).toBe("12.3K");
    expect(formatTokens(1_200_000)).toBe("1.2M");
  });
});
