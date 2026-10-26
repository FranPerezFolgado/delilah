// @vitest-environment jsdom
import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

describe("revealInFinder", () => {
  beforeEach(() => {
    invoke.mockReset();
    vi.resetModules();
    delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
  });

  it("does nothing outside the desktop app", async () => {
    const { isDesktop, revealInFinder } = await import("./platform");
    expect(isDesktop).toBe(false);
    await revealInFinder("/Users/x/.delilah");
    expect(invoke).not.toHaveBeenCalled();
  });

  it("calls reveal_path in the desktop app", async () => {
    (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
    const { revealInFinder } = await import("./platform");
    await revealInFinder("/Users/x/.delilah");
    expect(invoke).toHaveBeenCalledWith("reveal_path", { path: "/Users/x/.delilah" });
  });
});
