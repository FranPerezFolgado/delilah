// @vitest-environment jsdom
import { afterEach } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { OptionCard } from "./OptionCard";

it("is a radio named by its title with aria-checked", () => {
  render(
    <div role="radiogroup" aria-label="Origin">
      <OptionCard name="o" checked title="From scratch" description="Empty" />
      <OptionCard name="o" checked={false} title="From Jira" />
    </div>,
  );
  expect((screen.getByRole("radio", { name: /From scratch/ })).getAttribute("aria-checked")).toBe("true");
  expect((screen.getByRole("radio", { name: /From Jira/ })).getAttribute("aria-checked")).toBe("false");
  expect(screen.getByText("Empty")).toBeTruthy();
});
it("fires onClick", () => {
  const f = vi.fn();
  render(<OptionCard name="o" checked={false} title="A" onClick={f} />);
  fireEvent.click(screen.getByRole("radio"));
  expect(f).toHaveBeenCalled();
});
afterEach(cleanup);
