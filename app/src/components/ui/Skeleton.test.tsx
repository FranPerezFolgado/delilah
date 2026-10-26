// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render } from "@testing-library/react";
import { Skeleton } from "./Skeleton";

afterEach(cleanup);

describe("Skeleton", () => {
  it("is an aria-busy container with one line by default", () => {
    const { container } = render(<Skeleton />);
    expect(container.firstElementChild!.getAttribute("aria-busy")).toBe("true");
    expect(container.querySelectorAll(".sk").length).toBe(1);
  });
  it("renders the given number of lines", () => {
    const { container } = render(<Skeleton lines={4} />);
    expect(container.querySelectorAll(".sk").length).toBe(4);
  });
  it("applies the width", () => {
    const { container } = render(<Skeleton width={120} />);
    expect((container.querySelector(".sk") as HTMLElement).style.width).toBe("120px");
    cleanup();
    const r = render(<Skeleton width="60%" />);
    expect((r.container.querySelector(".sk") as HTMLElement).style.width).toBe("60%");
  });
});
