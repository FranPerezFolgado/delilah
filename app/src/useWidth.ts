// Breakpoints. The window ones (rail, overlays) come from media queries; the task header's
// compact mode depends on the width of the main area, not of the window, so it is measured.
import { type RefObject, useEffect, useState } from "react";

export const RAIL_BELOW = 1300;
export const OVERLAYS_BELOW = 1200;
/** The main area is compact at this width or less. */
export const COMPACT_MAIN = 960;

const query = (below: number) => `(max-width: ${below - 1}px)`;
const matches = (below: number) => typeof window !== "undefined" && !!window.matchMedia?.(query(below)).matches;

export function useBreakpoints(): { rail: boolean; overlays: boolean } {
  const [state, setState] = useState(() => ({ rail: matches(RAIL_BELOW), overlays: matches(OVERLAYS_BELOW) }));
  useEffect(() => {
    if (!window.matchMedia) return;
    const update = () =>
      setState((s) => {
        const next = { rail: matches(RAIL_BELOW), overlays: matches(OVERLAYS_BELOW) };
        return s.rail === next.rail && s.overlays === next.overlays ? s : next;
      });
    const mqs = [RAIL_BELOW, OVERLAYS_BELOW].map((b) => window.matchMedia(query(b)));
    mqs.forEach((m) => m.addEventListener("change", update));
    update();
    return () => mqs.forEach((m) => m.removeEventListener("change", update));
  }, []);
  return state;
}

/** The content width of the element, kept up to date with a ResizeObserver (Infinity until measured). */
export function useContainerWidth(ref: RefObject<HTMLElement | null>): number {
  const [width, setWidth] = useState(Infinity);
  useEffect(() => {
    const el = ref.current;
    if (!el || typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver((entries) => {
      const w = entries[entries.length - 1]?.contentRect.width;
      if (w !== undefined) setWidth(w);
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, [ref]);
  return width;
}
