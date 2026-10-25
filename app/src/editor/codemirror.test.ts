// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { EditorView } from "@codemirror/view";
import { createState, themeSlot, themeFor } from "./codemirror";

type Listener = (e: { matches: boolean }) => void;

function mockMatchMedia(matches: boolean) {
  const listeners = new Set<Listener>();
  vi.stubGlobal("matchMedia", () => ({
    matches,
    addEventListener: (_: string, l: Listener) => listeners.add(l),
    removeEventListener: (_: string, l: Listener) => listeners.delete(l),
  }));
  return (m: boolean) => listeners.forEach((l) => l({ matches: m }));
}

afterEach(() => vi.unstubAllGlobals());

it("reconfigures the theme compartment when the scheme changes", () => {
  const fire = mockMatchMedia(true);
  const view = new EditorView({
    state: createState("x", { onSave() {}, onChange() {} }),
    parent: document.body,
  });
  const dark = themeSlot.get(view.state);
  expect(dark).toBe(themeFor(true));
  fire(false);
  const light = themeSlot.get(view.state);
  expect(light).toBe(themeFor(false));
  expect(light).not.toBe(dark);
  fire(true);
  expect(themeSlot.get(view.state)).toBe(themeFor(true));
  view.destroy();
  fire(false);
  expect(themeSlot.get(view.state)).toBe(themeFor(true));
});
