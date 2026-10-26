// @vitest-environment jsdom
import { afterEach } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { Button, IconButton } from "./Button";

describe("Button", () => {
  it.each([["primary", "b-p"], ["secondary", "b-s"], ["ghost", "b-g"], ["danger", "b-d"]] as const)("%s -> %s", (v, c) => {
    render(<Button variant={v}>Go</Button>);
    const b = screen.getByRole("button", { name: "Go" });
    expect(b.className).toContain("b ");
    expect((b).classList.contains(c)).toBe(true);
    expect((b).classList.contains("sm")).toBe(false);
  });
  it("size 28 adds sm", () => {
    render(<Button variant="ghost" size={28}>Go</Button>);
    expect((screen.getByRole("button")).classList.contains("sm")).toBe(true);
  });
  it("is type=button, fires onClick, and disabled keeps its title", () => {
    const f = vi.fn();
    const { rerender } = render(<Button variant="primary" onClick={f}>Go</Button>);
    fireEvent.click(screen.getByRole("button"));
    expect(f).toHaveBeenCalledTimes(1);
    expect((screen.getByRole("button")).getAttribute("type")).toBe("button");
    rerender(<Button variant="primary" disabled title="No repo">Go</Button>);
    expect((screen.getByRole("button")).hasAttribute("disabled")).toBe(true);
    expect((screen.getByRole("button")).getAttribute("title")).toBe("No repo");
  });
  it("renders a decorative icon", () => {
    const { container } = render(<Button variant="primary" icon="plus">Add</Button>);
    expect((container.querySelector("svg")!).getAttribute("aria-hidden")).toBe("true");
  });
});

describe("IconButton", () => {
  it("is named by its label", () => {
    render(<IconButton label="Close" icon="close" />);
    const b = screen.getByRole("button", { name: "Close" });
    expect((b).classList.contains("ib")).toBe(true);
    expect((b).hasAttribute("aria-pressed")).toBe(false);
  });
  it("sets aria-pressed only when pressed is given", () => {
    const { rerender } = render(<IconButton label="Rail" icon="rail" pressed={false} />);
    expect((screen.getByRole("button")).getAttribute("aria-pressed")).toBe("false");
    rerender(<IconButton label="Rail" icon="rail" pressed />);
    expect(screen.getByRole("button", { pressed: true })).toBeTruthy();
  });
});
afterEach(cleanup);
