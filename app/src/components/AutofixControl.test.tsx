// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { setLanguage } from "../i18n";
import { AutofixControl, autofixDefault } from "./AutofixControl";

beforeEach(() => setLanguage("es"));
afterEach(cleanup);

const toggle = () => screen.getByRole("button", { name: /Arreglar CI/ }) as HTMLButtonElement;

it("shows on and off, and a click asks for the opposite", () => {
  const onChange = vi.fn();
  const { rerender } = render(<AutofixControl on={false} onChange={onChange} />);
  expect(toggle().getAttribute("aria-pressed")).toBe("false");
  fireEvent.click(toggle());
  expect(onChange).toHaveBeenLastCalledWith(true);
  rerender(<AutofixControl on onChange={onChange} />);
  expect(toggle().getAttribute("aria-pressed")).toBe("true");
  fireEvent.click(toggle());
  expect(onChange).toHaveBeenLastCalledWith(false);
});

it("is disabled on a finished task and while busy", () => {
  const onChange = vi.fn();
  const { rerender } = render(<AutofixControl on onChange={onChange} disabled />);
  expect(toggle().disabled).toBe(true);
  fireEvent.click(toggle());
  expect(onChange).not.toHaveBeenCalled();
  rerender(<AutofixControl on onChange={onChange} busy />);
  expect(toggle().disabled).toBe(true);
});

it("the default follows the project mode and autopilot", () => {
  expect(autofixDefault("off", true)).toBe(false);
  expect(autofixDefault("autopilot", true)).toBe(true);
  expect(autofixDefault("autopilot", false)).toBe(false);
  expect(autofixDefault(undefined, true)).toBe(true);
  expect(autofixDefault("always", false)).toBe(true);
});
