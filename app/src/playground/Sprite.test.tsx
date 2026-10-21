// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render } from "@testing-library/react";
import { Sprite } from "./Sprite";

afterEach(cleanup);

const paths = (c: HTMLElement) => [...c.querySelectorAll("path")];

describe("Sprite", () => {
  it("renders an svg with one path per colour and frame", () => {
    const { container } = render(<Sprite frames={[["ab", "bb"]]} />);
    const svg = container.querySelector("svg")!;
    expect(svg.getAttribute("shape-rendering")).toBe("crispEdges");
    expect(paths(container).map((p) => p.getAttribute("fill"))).toEqual(["var(--pg-a)", "var(--pg-b)"]);
    expect(paths(container)[0].getAttribute("d")).toBe("M0 0h1v1h-1z");
    expect(paths(container)[1].getAttribute("d")).toBe("M1 0h1v1h-1zM0 1h2v1h-2z");
  });

  it("draws nothing for '.'", () => {
    const { container } = render(<Sprite frames={[["..", ".."]]} />);
    expect(paths(container)).toHaveLength(0);
  });

  it("puts frames side by side and sets the frame count and width", () => {
    const { container } = render(<Sprite frames={[["a."], ["b."]]} />);
    const svg = container.querySelector("svg")!;
    expect(svg.getAttribute("viewBox")).toBe("0 0 2 1");
    expect(paths(container).map((p) => p.getAttribute("d"))).toEqual(["M0 0h1v1h-1z", "M2 0h1v1h-1z"]);
    expect(svg.style.getPropertyValue("--pg-fn")).toBe("2");
    expect(svg.style.getPropertyValue("--pg-fw")).toBe("2px");
  });

  it("never sets a single-letter --pg- property, which belongs to the palette", () => {
    const { container } = render(<Sprite frames={[["a."], ["b."]]} />);
    const style = container.querySelector("svg")!.getAttribute("style") ?? "";
    expect(style).not.toMatch(/--pg-[a-z]\s*:/);
  });
});
