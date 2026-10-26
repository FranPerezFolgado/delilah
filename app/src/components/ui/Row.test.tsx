// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Row } from "./Row";

afterEach(cleanup);

describe("Row", () => {
  it("is a button with leading and trailing slots", () => {
    const onClick = vi.fn();
    render(<Row leading={<i data-testid="l" />} trailing={<b data-testid="t" />} onClick={onClick} title="Tarea" />);
    const el = screen.getByRole("button");
    expect(el.className).toContain("row");
    expect(el.contains(screen.getByTestId("l"))).toBe(true);
    expect(el.contains(screen.getByTestId("t"))).toBe(true);
    fireEvent.click(el);
    expect(onClick).toHaveBeenCalled();
  });

  it("marks current and selected", () => {
    render(<Row current selected title="x" />);
    const el = screen.getByRole("button");
    expect(el.getAttribute("aria-current")).toBe("true");
    expect(el.getAttribute("aria-selected")).toBe("true");
  });

  it("omits the aria attributes when not current or selected", () => {
    render(<Row title="x" />);
    const el = screen.getByRole("button");
    expect(el.hasAttribute("aria-current")).toBe(false);
    expect(el.hasAttribute("aria-selected")).toBe(false);
  });

  it("renders as a link and adds the two-line variant with a sub line", () => {
    render(<Row as="a" href="#/x" title="Título" sub="detalle" />);
    const el = screen.getByRole("link");
    expect(el.className).toContain("row-2");
    expect(el.textContent).toContain("Título");
    expect(el.textContent).toContain("detalle");
  });
});
