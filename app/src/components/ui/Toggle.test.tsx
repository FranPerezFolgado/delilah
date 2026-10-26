// @vitest-environment jsdom
import { afterEach } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { Toggle } from "./Toggle";

it("reflects pressed and reports the opposite on click", () => {
  const f = vi.fn();
  render(<Toggle label="Autopilot" pressed={false} onChange={f} />);
  const b = screen.getByRole("button", { name: "Autopilot" });
  expect((b).getAttribute("aria-pressed")).toBe("false");
  fireEvent.click(b);
  expect(f).toHaveBeenCalledWith(true);
});
it("pressed -> onChange(false)", () => {
  const f = vi.fn();
  render(<Toggle label="Saving" pressed onChange={f} />);
  expect(screen.getByRole("button", { pressed: true })).toBeTruthy();
  fireEvent.click(screen.getByRole("button"));
  expect(f).toHaveBeenCalledWith(false);
});
it("iconOnly keeps the accessible name without visible text", () => {
  render(<Toggle label="Saving" pressed={false} onChange={() => {}} iconOnly />);
  const b = screen.getByRole("button", { name: "Saving" });
  expect(b.textContent).toBe("");
});
it("disabled does not fire and shows its reason", () => {
  const f = vi.fn();
  render(<Toggle label="Autopilot" pressed={false} onChange={f} disabled title="Not available" />);
  const b = screen.getByRole("button");
  expect((b).hasAttribute("disabled")).toBe(true);
  expect((b).getAttribute("title")).toBe("Not available");
  fireEvent.click(b);
  expect(f).not.toHaveBeenCalled();
});
afterEach(cleanup);
