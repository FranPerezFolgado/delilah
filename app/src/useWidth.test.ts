// @vitest-environment jsdom
import { createElement, useRef } from "react";
import { act, cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { COMPACT_MAIN, useBreakpoints, useContainerWidth } from "./useWidth";

let listeners: Array<() => void> = [];
let width = 1400;
beforeEach(() => {
  listeners = [];
  width = 1400;
  window.matchMedia = ((q: string) => {
    const max = Number(/max-width:\s*(\d+)/.exec(q)?.[1]);
    return {
      get matches() { return width <= max; },
      media: q,
      addEventListener: (_: string, cb: () => void) => listeners.push(cb),
      removeEventListener: (_: string, cb: () => void) => { listeners = listeners.filter((l) => l !== cb); },
    } as unknown as MediaQueryList;
  }) as typeof window.matchMedia;
});
afterEach(cleanup);

function Bp({ out }: { out: { v?: ReturnType<typeof useBreakpoints> } }) {
  out.v = useBreakpoints();
  return null;
}

describe("useBreakpoints", () => {
  it("rail < 1300 and overlays < 1200", () => {
    const out: { v?: ReturnType<typeof useBreakpoints> } = {};
    render(createElement(Bp, { out }));
    expect(out.v).toEqual({ rail: false, overlays: false });
    act(() => { width = 1250; listeners.forEach((l) => l()); });
    expect(out.v).toEqual({ rail: true, overlays: false });
    act(() => { width = 1100; listeners.forEach((l) => l()); });
    expect(out.v).toEqual({ rail: true, overlays: true });
  });
});

describe("useContainerWidth", () => {
  it("follows the element through a ResizeObserver", () => {
    let fire: (w: number) => void = () => {};
    const disconnect = vi.fn();
    class RO {
      constructor(cb: ResizeObserverCallback) {
        fire = (w) => cb([{ contentRect: { width: w } } as ResizeObserverEntry], this as unknown as ResizeObserver);
      }
      observe() {}
      unobserve() {}
      disconnect = disconnect;
    }
    vi.stubGlobal("ResizeObserver", RO);
    const out: { w?: number; compact?: boolean } = {};
    function C() {
      const ref = useRef<HTMLDivElement>(null);
      const w = useContainerWidth(ref);
      out.w = w;
      out.compact = w <= COMPACT_MAIN;
      return createElement("div", { ref });
    }
    const r = render(createElement(C));
    act(() => fire(1000));
    expect(out.compact).toBe(false);
    act(() => fire(960));
    expect(out.w).toBe(960);
    expect(out.compact).toBe(true);
    r.unmount();
    expect(disconnect).toHaveBeenCalled();
    vi.unstubAllGlobals();
  });
});
