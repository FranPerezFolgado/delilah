// @vitest-environment jsdom
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { Action } from "../api";
import { KIND_COLOURS } from "./art/kinds";
import { setLanguage } from "../i18n";
import { Floor } from "./Floor";
import type { DeskSeat, FloorModel } from "./useOffice";

beforeAll(() => setLanguage("es"));

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

const action = (kind: Action["kind"] = "edit", target: string | null = "api.rs"): Action => ({
  id: "a", kind, tool: "Edit", target, background: false, since: "2026-01-01T00:00:00Z",
});
const lead = (slot: number, over: Partial<DeskSeat> = {}): DeskSeat => ({
  slot, session: `s${slot}`, state: "working", action: null, lookKey: "proj", ...over,
});
const helper = (slot: number, over: Partial<DeskSeat> = {}): DeskSeat => ({
  slot, helperId: `h${slot}`, state: "working", action: null, lookKey: "code-researcher", ...over,
});
const model = (over: Partial<FloorModel> = {}): FloorModel => ({
  owner: "LIBRE-1", title: "Add thing", phase: null, project: "proj", leads: [], helpers: [], leaving: [], waiting: 0, ...over,
});
const show = (floor: FloorModel, online = true, onOpen = () => {}) =>
  render(<Floor floor={floor} online={online} onOpenSession={onOpen} />);

describe("Floor", () => {
  it("draws a desk per lead and helper", () => {
    const { container } = show(model({ leads: [lead(0), lead(1)], helpers: [helper(2)] }));
    expect(container.querySelectorAll(".fl-desk")).toHaveLength(3);
    expect(screen.getAllByRole("button")).toHaveLength(2);
    expect(screen.getAllByRole("img")).toHaveLength(1);
  });

  it("colours the glass by the action kind", () => {
    const { container } = show(model({ leads: [lead(0, { action: action("run") })] }));
    expect(container.querySelector(".fl-halo")?.getAttribute("data-glow")).toBe(KIND_COLOURS.run);
    expect(container.querySelector(".fl-desk")?.getAttribute("data-kind")).toBe("run");
  });

  it("shows ? when waiting and zzz when paused", () => {
    const { container } = show(model({ leads: [lead(0, { state: "waiting" }), lead(1, { state: "paused" })], waiting: 1 }));
    expect(container.querySelector(".fl-bubble--wait")?.textContent).toBe("?");
    expect(container.querySelector(".fl-bubble--zzz")?.textContent).toBe("zzz");
    expect(container.querySelector(".fl-pill")?.textContent).toContain("1");
  });

  it("shows a token tower by tier with hover text", () => {
    const { container } = show(model({ leads: [lead(0, { tokens: 3_000_000 }), lead(1, { tokens: 100 })] }));
    const towers = container.querySelectorAll(".fl-tower");
    expect(towers).toHaveLength(1);
    expect(towers[0].getAttribute("data-tier")).toBe("2");
    expect(towers[0].getAttribute("title")).toBe("3M tokens");
  });

  it("names the lead button and the helper", () => {
    show(model({ leads: [lead(0, { action: action(), tokens: 1_200_000 })], helpers: [helper(1, { action: action("read") })] }));
    const name = screen.getByRole("button").getAttribute("aria-label")!;
    expect(name).toContain("LIBRE-1");
    expect(name).toContain("Edit · api.rs");
    expect(name).toContain("1.2M tokens");
    expect(screen.getByRole("img").getAttribute("aria-label")).toContain("Edit · api.rs");
  });

  it("shows the empty state", () => {
    show(model());
    expect(screen.getByText("Ningún agente trabajando en esta tarea")).toBeTruthy();
  });

  it("opens the session on click", () => {
    const onOpen = vi.fn();
    show(model({ leads: [lead(0)] }), true, onOpen);
    fireEvent.click(screen.getByRole("button"));
    expect(onOpen).toHaveBeenCalledWith("s0");
  });

  it("marks the floor stale offline", () => {
    const { container } = show(model({ leads: [lead(0)] }), false);
    expect(container.querySelector(".fl--stale")).toBeTruthy();
    expect(screen.getByRole("status")).toBeTruthy();
  });

  it("walks a leaver to the lounge, then it is gone", () => {
    vi.useFakeTimers();
    const floor = model({ leads: [lead(0)], leaving: [{ id: "h1", slot: 1, since: Date.now() }] });
    const { container, rerender } = show(floor);
    expect(container.querySelector(".fl-leaver")?.getAttribute("data-step")).toBe("0");
    act(() => {
      vi.advanceTimersByTime(1500);
    });
    expect(container.querySelector(".fl-leaver")?.getAttribute("data-step")).toBe("2");
    rerender(<Floor floor={{ ...floor, leaving: [] }} online onOpenSession={() => {}} />);
    expect(container.querySelector(".fl-leaver")).toBeNull();
  });
});
