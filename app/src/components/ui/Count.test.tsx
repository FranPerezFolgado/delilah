// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render } from "@testing-library/react";
import { readFileSync } from "node:fs";
import { Count } from "./Count";

afterEach(cleanup);

describe("Count", () => {
  it("shows the number, neutral by default", () => {
    const { container } = render(<Count n={3} />);
    const el = container.querySelector(".cnt")!;
    expect(el.textContent).toBe("3");
    expect(el.classList.contains("hot")).toBe(false);
  });
  it("hot adds the hot class", () => {
    const { container } = render(<Count n={2} hot />);
    expect(container.querySelector(".cnt.hot")).not.toBeNull();
  });
  it("is hidden at zero unless showZero", () => {
    expect(render(<Count n={0} />).container.firstChild).toBeNull();
    cleanup();
    expect(render(<Count n={0} showZero />).container.textContent).toBe("0");
  });
  it("hot text is --wait-on on --wait", () => {
    const css = readFileSync("src/components/ui/Count.css", "utf8");
    expect(css).toMatch(/\.cnt\.hot\s*\{[^}]*background:\s*var\(--wait\)[^}]*color:\s*var\(--wait-on\)/);
  });
});
