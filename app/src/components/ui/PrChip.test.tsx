// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { cleanup, render } from "@testing-library/react";
import { setLanguage } from "../../i18n";
beforeEach(() => setLanguage("es"));
afterEach(() => { cleanup(); localStorage.clear(); });

import { readFileSync } from "node:fs";
import { PrChip } from "./PrChip";
import type { PrSnapshot } from "../../api";

const snap = (o: Partial<PrSnapshot>) => ({ state: "open", draft: false, ci: "none", ...o }) as PrSnapshot;

describe("PrChip", () => {
  it.each([
    [snap({ draft: true }), "draft", "Borrador"],
    [snap({}), "open", "Abierta"],
    [snap({ ci: "failure" }), "failing", "Checks fallan"],
    [snap({ state: "merged" }), "merged", "Fusionada"],
  ])("%j -> %s", (snapshot, cls, text) => {
    const { container } = render(<PrChip pr={{ number: 9, snapshot }} />);
    const el = container.querySelector(".prchip")!;
    expect(el.classList.contains(cls)).toBe(true);
    expect(el.textContent).toBe(`#9${text}`);
    expect(el.querySelector("svg")).not.toBeNull();
  });
  it("accepts a number and a state", () => {
    const { container } = render(<PrChip pr={{ number: 3, state: "merged" }} />);
    expect(container.textContent).toBe("#3Fusionada");
  });
  it("Abierta is neutral and the radius is 7", () => {
    const css = readFileSync("src/components/ui/PrChip.css", "utf8");
    expect(css).toMatch(/\.prchip\s*\{[^}]*border-radius:\s*7px/);
    expect(css).not.toMatch(/\.prchip\.open/);
  });
});
