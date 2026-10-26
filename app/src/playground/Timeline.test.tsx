// @vitest-environment jsdom
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { TimelineRow } from "./flowModel";
import { setLanguage } from "../i18n";
import { Timeline } from "./Timeline";

beforeAll(() => setLanguage("es"));

const row = (seq: number, over: Partial<TimelineRow> = {}): TimelineRow => ({
  seq,
  at: new Date(2026, 0, 1, 10, 42, 5 + seq).toISOString(),
  node: `n${seq}`,
  session: "s",
  event: "tool_started",
  kind: "edit",
  tool: "Edit",
  target: "api.rs",
  ...over,
});

afterEach(cleanup);

describe("Timeline", () => {
  it("renders rows in order with time, kind, tool and target", () => {
    render(<Timeline rows={[row(0), row(1, { kind: "read", tool: "Read" })]} selected={null} onSelect={() => {}} />);
    const opts = screen.getAllByRole("option");
    expect(opts).toHaveLength(2);
    expect(opts[0].textContent).toContain("10:42:05");
    expect(opts[0].textContent).toContain("Edición");
    expect(opts[0].textContent).toContain("api.rs");
    expect(opts[1].textContent).toContain("Lectura");
  });

  it("renders nothing without rows", () => {
    const { container } = render(<Timeline rows={[]} selected={null} onSelect={() => {}} />);
    expect(container.firstChild).toBeNull();
  });

  it("click selects the node", () => {
    const onSelect = vi.fn();
    render(<Timeline rows={[row(0), row(1)]} selected={null} onSelect={onSelect} />);
    fireEvent.click(screen.getAllByRole("option")[1]);
    expect(onSelect).toHaveBeenCalledWith("n1");
  });

  it("marks the selected row", () => {
    render(<Timeline rows={[row(0), row(1)]} selected="n1" onSelect={() => {}} />);
    const opts = screen.getAllByRole("option");
    expect(opts[0].getAttribute("aria-selected")).toBe("false");
    expect(opts[1].getAttribute("aria-selected")).toBe("true");
  });

  it("keys move the selection", () => {
    const onSelect = vi.fn();
    render(<Timeline rows={[row(0), row(1), row(2)]} selected="n1" onSelect={onSelect} />);
    const opts = screen.getAllByRole("option");
    fireEvent.keyDown(opts[1], { key: "ArrowDown" });
    expect(onSelect).toHaveBeenLastCalledWith("n2");
    fireEvent.keyDown(opts[1], { key: "ArrowUp" });
    expect(onSelect).toHaveBeenLastCalledWith("n0");
    fireEvent.keyDown(opts[1], { key: "End" });
    expect(onSelect).toHaveBeenLastCalledWith("n2");
    fireEvent.keyDown(opts[1], { key: "Home" });
    expect(onSelect).toHaveBeenLastCalledWith("n0");
  });

  it("marks failed rows and names them", () => {
    render(<Timeline rows={[row(0, { event: "tool_ended", failed: true })]} selected={null} onSelect={() => {}} />);
    const o = screen.getByRole("option");
    expect(o.className).toContain("failed");
    expect(o.getAttribute("aria-label")).toContain("fallida");
    expect(o.getAttribute("aria-label")).toContain("10:42:05");
  });
});
