// @vitest-environment jsdom
import { afterEach } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { Segmented } from "./Segmented";

const opts = [
  { value: "chat", label: "Chat", tag: "beta" },
  { value: "term", label: "Terminal" },
  { value: "x", label: "Off", disabled: true },
];

it("radio mode: radiogroup, radios, aria-checked, roving tabindex", () => {
  render(<Segmented mode="radio" label="View" options={opts} value="term" onChange={() => {}} />);
  expect(screen.getByRole("radiogroup", { name: "View" })).toBeTruthy();
  expect((screen.getByRole("radio", { name: "Terminal" })).getAttribute("aria-checked")).toBe("true");
  expect((screen.getByRole("radio", { name: /Chat/ })).getAttribute("aria-checked")).toBe("false");
  expect((screen.getByRole("radio", { name: /Chat/ })).getAttribute("tabindex")).toBe("-1");
});
it("radio mode: arrows move and wrap, skipping disabled", () => {
  const f = vi.fn();
  render(<Segmented mode="radio" label="View" options={opts} value="term" onChange={f} />);
  fireEvent.keyDown(screen.getByRole("radio", { name: "Terminal" }), { key: "ArrowRight" });
  expect(f).toHaveBeenLastCalledWith("chat");
  fireEvent.keyDown(screen.getByRole("radio", { name: "Terminal" }), { key: "ArrowLeft" });
  expect(f).toHaveBeenLastCalledWith("chat");
});
it("pressed mode: buttons with aria-pressed", () => {
  const f = vi.fn();
  render(<Segmented mode="pressed" label="Mode" options={opts} value="chat" onChange={f} />);
  expect(screen.queryByRole("radio")).toBeNull();
  expect(screen.getByRole("button", { name: /Chat/, pressed: true })).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Terminal" }));
  expect(f).toHaveBeenCalledWith("term");
});
it("renders the beta tag and disabled options", () => {
  render(<Segmented mode="pressed" label="Mode" options={opts} value="chat" onChange={() => {}} />);
  expect(screen.getByText("beta")).toBeTruthy();
  expect((screen.getByRole("button", { name: "Off" })).hasAttribute("disabled")).toBe(true);
});
afterEach(cleanup);
