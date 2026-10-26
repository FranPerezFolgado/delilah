// The app's colours live in CSS custom properties that follow the OS scheme; libraries
// that take colours as values (xterm, CodeMirror) read them here and re-read on change.

const QUERY = "(prefers-color-scheme: dark)";

/** The current value of a CSS custom property on the root, e.g. `cssVar("--term-bg")`. */
export function cssVar(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

/** Whether the OS is in dark mode right now. */
export function isDark(): boolean {
  return typeof matchMedia !== "undefined" && matchMedia(QUERY).matches;
}

/** Calls `cb(dark)` whenever the OS scheme changes; returns the unsubscribe. */
export function onSchemeChange(cb: (dark: boolean) => void): () => void {
  if (typeof matchMedia === "undefined") return () => {};
  const mq = matchMedia(QUERY);
  const handler = (e: { matches: boolean }) => cb(e.matches);
  mq.addEventListener("change", handler);
  return () => mq.removeEventListener("change", handler);
}
