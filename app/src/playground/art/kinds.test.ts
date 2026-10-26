import { describe, it, expect } from "vitest";
import { KIND_COLOURS, tokenTier, formatTokens } from "./kinds";

describe("kinds", () => {
  it("colours", () => {
    expect(KIND_COLOURS).toEqual({
      edit: "#64a0ff",
      read: "#50dcf0",
      run: "#f0aa50",
      delegate: "#c88cff",
      other: "#8cf0aa",
    });
  });
  it("tokenTier thresholds", () => {
    expect(tokenTier(0)).toBe(0);
    expect(tokenTier(249_999)).toBe(0);
    expect(tokenTier(250_000)).toBe(1);
    expect(tokenTier(1_999_999)).toBe(1);
    expect(tokenTier(2_000_000)).toBe(2);
    expect(tokenTier(15_999_999)).toBe(2);
    expect(tokenTier(16_000_000)).toBe(3);
  });
  it("formatTokens", () => {
    expect(formatTokens(816)).toBe("816");
    expect(formatTokens(9500)).toBe("9.5K");
    expect(formatTokens(250_000)).toBe("250K");
    expect(formatTokens(2_400_000)).toBe("2.4M");
    expect(formatTokens(16_000_000)).toBe("16M");
  });
});
