// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render } from "@testing-library/react";
import { PixelSprite, spritePaths } from "./PixelSprite";
import { ANIMATIONS, PALETTE, SPRITES } from "./art/pixtuoid";

afterEach(cleanup);

describe("spritePaths", () => {
  it("merges row runs and groups by colour", () => {
    const out = spritePaths(["aab", "b.a"], { a: "#111", b: "#222" });
    expect(out).toEqual([
      { fill: "#111", d: "M0 0h2v1h-2zM2 1h1v1h-1z" },
      { fill: "#222", d: "M2 0h1v1h-1zM0 1h1v1h-1z" },
    ]);
  });

  it("applies the y offset and skips transparent", () => {
    expect(spritePaths(["a."], { a: "#111" }, 2)).toEqual([{ fill: "#111", d: "M0 2h1v1h-1z" }]);
  });

  it("merges keys that resolve to the same colour", () => {
    expect(spritePaths(["ab"], { a: "#111", b: "#111" })).toEqual([{ fill: "#111", d: "M0 0h2v1h-2z" }]);
  });
});

describe("PixelSprite", () => {
  it("renders one path per distinct colour, scaled, aria-hidden", () => {
    const s = SPRITES.seated;
    const colours = new Set(s.frames[0].join("").split("").filter((c) => c !== "." && PALETTE[c]).map((c) => PALETTE[c]));
    const { container } = render(<PixelSprite sprite="seated" scale={3} className="x" />);
    const svg = container.querySelector("svg")!;
    expect(container.querySelectorAll("path")).toHaveLength(colours.size);
    expect(svg.getAttribute("width")).toBe(String(s.w * 3));
    expect(svg.getAttribute("height")).toBe(String(s.h * 3));
    expect(svg.getAttribute("viewBox")).toBe(`0 0 ${s.w} ${s.h}`);
    expect(svg.getAttribute("aria-hidden")).toBe("true");
    expect(svg.getAttribute("class")).toContain("x");
  });

  it("applies recolour overrides", () => {
    const { container } = render(<PixelSprite sprite="seated" scale={1} recolour={{ B: "#123456" }} />);
    const fills = [...container.querySelectorAll("path")].map((p) => p.getAttribute("fill"));
    expect(fills).toContain("#123456");
    expect(fills).not.toContain(PALETTE.B);
  });

  it("renders all frames of an animation", () => {
    const a = ANIMATIONS.typing;
    const { container } = render(<PixelSprite animation="typing" scale={2} />);
    const gs = container.querySelectorAll("g.pg-frame");
    expect(gs).toHaveLength(a.frames.length);
    expect((gs[1] as SVGElement).style.getPropertyValue("--pg-delay")).toBe(`-${a.frameMs}ms`);
    expect((gs[0] as SVGElement).style.getPropertyValue("--pg-dur")).toBe(`${a.frames.length * a.frameMs}ms`);
  });

  it("renders a static sprite without animated groups", () => {
    const { container } = render(<PixelSprite sprite="seated" scale={1} />);
    expect(container.querySelectorAll(".pg-frame")).toHaveLength(0);
  });
});
