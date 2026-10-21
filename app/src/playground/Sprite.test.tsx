// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render } from "@testing-library/react";
import { Sprite } from "./Sprite";

afterEach(cleanup);

describe("Sprite", () => {
  it("renders an svg with one rect per horizontal run of one colour", () => {
    const { container } = render(<Sprite frames={[["ab", "bb"]]} />);
    const svg = container.querySelector("svg")!;
    expect(svg.getAttribute("shape-rendering")).toBe("crispEdges");
    expect(svg.querySelectorAll("rect")).toHaveLength(3);
  });

  it("merges runs and skips transparent pixels", () => {
    const { container } = render(<Sprite frames={[["a.", "aa"]]} />);
    const rects = [...container.querySelectorAll("rect")];
    expect(rects).toHaveLength(2);
    expect(rects.every((r) => r.getAttribute("fill") === "var(--pg-a)")).toBe(true);
    expect(rects.map((r) => r.getAttribute("width"))).toEqual(["1", "2"]);
  });

  it("uses the palette variable per key", () => {
    const { container } = render(<Sprite frames={[["ab"]]} />);
    expect([...container.querySelectorAll("rect")].map((r) => r.getAttribute("fill"))).toEqual(["var(--pg-a)", "var(--pg-b)"]);
  });

  it("puts frames side by side and sets --pg-frames", () => {
    const { container } = render(<Sprite frames={[["a."], ["b."]]} />);
    const svg = container.querySelector("svg")!;
    expect(svg.getAttribute("viewBox")).toBe("0 0 2 1");
    const xs = [...svg.querySelectorAll("rect")].map((r) => r.getAttribute("x"));
    expect(xs).toEqual(["0", "2"]);
    expect((container.firstElementChild as HTMLElement).style.getPropertyValue("--pg-frames")).toBe("2");
  });
});
