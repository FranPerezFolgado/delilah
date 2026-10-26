// @vitest-environment jsdom
import { afterEach } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { expect, it } from "vitest";
import { IconButton } from "./Button";

it("IconButton keeps the icon svg hidden from the accessibility tree", () => {
  const { container } = render(<IconButton label="Logs" icon="logs" />);
  expect((container.querySelector("svg")!).getAttribute("aria-hidden")).toBe("true");
  expect(screen.getByLabelText("Logs")).toBeTruthy();
});
afterEach(cleanup);
