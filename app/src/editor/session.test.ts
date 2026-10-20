// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { clampTreeWidth, setTreeWidth, TREE_WIDTH, treeWidth } from "./session";

const KEY = "delilah.tree-width";

describe("clampTreeWidth", () => {
  it("keeps a width within the limits as it is", () => {
    expect(clampTreeWidth(300)).toBe(300);
  });

  it("holds the tree between its minimum and maximum", () => {
    expect(clampTreeWidth(10)).toBe(TREE_WIDTH.min);
    expect(clampTreeWidth(5000)).toBe(TREE_WIDTH.max);
  });

  it("leaves the main column its room on the screen", () => {
    expect(clampTreeWidth(600, 800)).toBe(800 - TREE_WIDTH.mainMin);
  });

  it("lets the tree's minimum win on a screen too narrow for both", () => {
    expect(clampTreeWidth(300, 400)).toBe(TREE_WIDTH.min);
  });

  it("rounds to whole pixels", () => {
    expect(clampTreeWidth(250.6)).toBe(251);
  });
});

describe("treeWidth", () => {
  afterEach(() => {
    localStorage.clear();
    vi.restoreAllMocks();
  });

  it("is the default when nothing is stored", () => {
    expect(treeWidth()).toBe(TREE_WIDTH.default);
  });

  it("reads back what setTreeWidth stored, rounded", () => {
    setTreeWidth(333.4);
    expect(localStorage.getItem(KEY)).toBe("333");
    expect(treeWidth()).toBe(333);
  });

  it.each(["abc", "", "100", "9999"])("ignores a stored %j", (stored) => {
    localStorage.setItem(KEY, stored);
    expect(treeWidth()).toBe(TREE_WIDTH.default);
  });

  it("survives storage that throws", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("denied");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("denied");
    });
    expect(treeWidth()).toBe(TREE_WIDTH.default);
    expect(() => setTreeWidth(300)).not.toThrow();
  });
});
