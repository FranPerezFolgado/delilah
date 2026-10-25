// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { cssVar, isDark, onSchemeChange } from "./theme";

type Listener = (e: { matches: boolean }) => void;

function mockMatchMedia(matches: boolean) {
  const listeners = new Set<Listener>();
  vi.stubGlobal("matchMedia", () => ({
    matches,
    addEventListener: (_: string, l: Listener) => listeners.add(l),
    removeEventListener: (_: string, l: Listener) => listeners.delete(l),
  }));
  return { listeners, fire: (m: boolean) => listeners.forEach((l) => l({ matches: m })) };
}

afterEach(() => {
  vi.unstubAllGlobals();
  document.documentElement.style.cssText = "";
});

it("cssVar reads a custom property from the root", () => {
  document.documentElement.style.setProperty("--term-bg", " #123456 ");
  expect(cssVar("--term-bg")).toBe("#123456");
});

it("isDark follows matchMedia", () => {
  mockMatchMedia(true);
  expect(isDark()).toBe(true);
});

it("onSchemeChange calls back on a change and stops after unsubscribe", () => {
  const mm = mockMatchMedia(false);
  const seen: boolean[] = [];
  const off = onSchemeChange((d) => seen.push(d));
  mm.fire(true);
  mm.fire(false);
  expect(seen).toEqual([true, false]);
  off();
  expect(mm.listeners.size).toBe(0);
  mm.fire(true);
  expect(seen).toEqual([true, false]);
});
