// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { setLanguage } from "../i18n";
import { WherePicker } from "./WherePicker";

const proj = (workspace: "checkout" | "worktree" | null) =>
  ({ name: "p", path: "/x", kind: "single", settings: { workspace } }) as never;
const checked = (re: RegExp) => screen.getByRole("radio", { name: re }).getAttribute("aria-checked");

it("labels the checkout as the default and the worktree as the alternative", () => {
  render(<WherePicker value="checkout" onChange={() => {}} project={proj(null)} />);
  expect(screen.getByRole("radiogroup")).toBeTruthy();
  expect(screen.getByRole("radio", { name: /In your checkout · default/ })).toBeTruthy();
  expect(screen.getByRole("radio", { name: /Isolated \(worktree\)/ }).textContent).not.toMatch(/default/);
  expect(checked(/checkout/)).toBe("true");
  expect(checked(/Isolated/)).toBe("false");
});
it("marks the project's own choice as project default", () => {
  render(<WherePicker value="worktree" onChange={() => {}} project={proj("worktree")} />);
  expect(screen.getByRole("radio", { name: /Isolated \(worktree\) · project default/ })).toBeTruthy();
  expect(checked(/Isolated/)).toBe("true");
});
it("unset in the project settings selects the checkout and no longer says isolated", () => {
  const f = vi.fn();
  render(<WherePicker withDefault value={null} onChange={f} project={proj(null)} />);
  expect(screen.getAllByRole("radio")).toHaveLength(2);
  expect(checked(/checkout/)).toBe("true");
  expect(document.body.textContent).not.toMatch(/Default \(isolated\)/);
  fireEvent.click(screen.getByRole("radio", { name: /Isolated/ }));
  expect(f).toHaveBeenCalledWith("worktree");
  fireEvent.click(screen.getByRole("radio", { name: /checkout/ }));
  expect(f).toHaveBeenCalledWith(null);
});
beforeEach(() => setLanguage("en"));
beforeEach(() => setLanguage("en"));
afterEach(cleanup);
