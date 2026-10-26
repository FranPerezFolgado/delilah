// @vitest-environment jsdom
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { Action } from "../api";
import { KIND_COLOURS } from "./art/kinds";
import { OFFICE_COLOURS } from "./art/office";
import { LEAVER_WALK_MS, loungePath } from "./floorLayout";
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

  it("asks for a project in the empty text of a project floor", () => {
    show(model({ owner: "@web" }));
    expect(screen.getByText("Ningún agente trabajando en este proyecto")).toBeTruthy();
    expect(screen.queryByText(/tarea/)).toBeNull();
  });

  it("turns the monitor off on a paused desk whatever the last action", () => {
    const { container } = show(model({ leads: [lead(0, { state: "paused", action: action("run") })] }));
    expect(container.querySelector(".fl-halo")).toBeNull();
    expect(container.querySelector(".fl-desk")?.getAttribute("data-kind")).toBe("");
  });

  it("walks a starting lead in from beside its desk, then seats it", () => {
    vi.useFakeTimers();
    const { container } = show(model({ leads: [lead(0, { state: "starting" })] }));
    const walk = () => container.querySelector(".fl-char")?.getAttribute("data-walk");
    expect(walk()).toBe("walking");
    act(() => {
      vi.advanceTimersByTime(2000);
    });
    expect(walk()).toBe("seated");
  });

  it("the walk-in is an animation that ends on the chair: from beside the desk to no offset", () => {
    vi.useFakeTimers();
    const animate = vi.fn(() => ({ cancel: vi.fn() }));
    (HTMLElement.prototype as unknown as { animate: unknown }).animate = animate;
    try {
      const { container } = show(model({ leads: [lead(0, { state: "starting" })] }));
      expect(animate).toHaveBeenCalledTimes(1);
      const [frames] = animate.mock.calls[0] as unknown as [{ transform: string }[]];
      expect(frames[0].transform).not.toBe("translate(0, 0)");
      expect(frames[frames.length - 1].transform).toBe("translate(0, 0)");
      // The element's own position is the chair, so nothing but the animation moves it.
      expect((container.querySelector(".fl-char") as HTMLElement).style.transform).toBe("");
    } finally {
      delete (HTMLElement.prototype as unknown as { animate?: unknown }).animate;
    }
  });

  it("a working lead is seated at once", () => {
    const { container } = show(model({ leads: [lead(0)] }));
    expect(container.querySelector(".fl-char")?.getAttribute("data-walk")).toBe("seated");
  });

  it("with reduced motion a starting lead appears seated", () => {
    vi.stubGlobal("matchMedia", () => ({ matches: true }));
    const { container } = show(model({ leads: [lead(0, { state: "starting" })] }));
    expect(container.querySelector(".fl-char")?.getAttribute("data-walk")).toBe("seated");
    vi.unstubAllGlobals();
  });

  it("names a leaving helper by its role and gives it that role's outfit", () => {
    const base = { id: "h1", slot: 1, since: Date.now() };
    const { container, rerender } = show(model({ leads: [lead(0)], leaving: [{ ...base, role: "Explore" }] }));
    expect(screen.getByRole("img", { name: "Explore, se va" })).toBeTruthy();
    const fills = (c: HTMLElement) => c.querySelector(".fl-leaver")?.innerHTML;
    const explore = fills(container);
    rerender(<Floor floor={model({ leads: [lead(0)], leaving: [{ ...base, role: "Plan" }] })} online onOpenSession={() => {}} />);
    expect(fills(container)).not.toBe(explore);
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
    const leaver = () => container.querySelector(".fl-leaver");
    expect(leaver()?.getAttribute("data-step")).toBe("walking");
    act(() => {
      vi.advanceTimersByTime(LEAVER_WALK_MS + 100);
    });
    expect(leaver()?.getAttribute("data-step")).toBe("arrived");
    rerender(<Floor floor={{ ...floor, leaving: [] }} online onOpenSession={() => {}} />);
    expect(leaver()).toBeNull();
  });

  it("a leaver's own position is its lounge seat, so it cannot be stuck mid-floor", () => {
    vi.useFakeTimers();
    const dest = loungePath(1, 1).at(-1)!;
    const scale = 4;
    const expected = `translate(${(dest.x - 4) * scale}px, ${(dest.y - 12) * scale}px)`;
    const animate = vi.fn(() => ({ cancel: vi.fn() }));
    (HTMLElement.prototype as unknown as { animate: unknown }).animate = animate;
    try {
      const { container } = show(model({ leads: [lead(0)], leaving: [{ id: "h1", slot: 1, since: Date.now() }] }));
      const el = container.querySelector(".fl-leaver") as HTMLElement;
      expect(el.style.transform).toBe(expected);
      const [frames] = animate.mock.calls[0] as unknown as [{ transform: string; offset: number }[]];
      expect(frames[0].offset).toBe(0);
      expect(frames[frames.length - 1]).toMatchObject({ transform: expected, offset: 1 });
    } finally {
      delete (HTMLElement.prototype as unknown as { animate?: unknown }).animate;
    }
  });

  it("a leaver that is shown late (the floor opened mid-walk) starts where it should, not at its desk", () => {
    vi.useFakeTimers();
    const since = Date.now() - LEAVER_WALK_MS - 500;
    const { container } = show(model({ leads: [lead(0)], leaving: [{ id: "h1", slot: 1, since }] }));
    expect(container.querySelector(".fl-leaver")?.getAttribute("data-step")).toBe("arrived");
  });

  it("with reduced motion a leaver stands in the lounge at once", () => {
    vi.stubGlobal("matchMedia", () => ({ matches: true }));
    const { container } = show(model({ leads: [lead(0)], leaving: [{ id: "h1", slot: 1, since: Date.now() }] }));
    expect(container.querySelector(".fl-leaver")?.getAttribute("data-step")).toBe("still");
    vi.unstubAllGlobals();
  });

  it("draws the room: a wall band, a floor and wall decor", () => {
    const { container } = show(model({ leads: [lead(0)] }));
    expect(container.querySelector("svg.fl-bg pattern")).toBeTruthy();
    expect(container.querySelector("svg.fl-bg > rect")?.getAttribute("fill")).toBe(OFFICE_COLOURS.wall);
    // Whiteboard, bookshelf, clock, vending, pantry, cooler, sofa, table, plant, one desk and its cup.
    expect(container.querySelectorAll(".fl-floor svg.pg-sprite").length).toBeGreaterThanOrEqual(11);
  });

  it("seats the character behind the desk: its head is above the desk's top edge", () => {
    const { container } = show(model({ leads: [lead(0)] }));
    const char = container.querySelector(".fl-char") as HTMLElement;
    const desk = container.querySelector(".fl-desk svg.fl-abs") as unknown as SVGElement;
    const scale = Number(container.querySelector(".fl-floor")!.getAttribute("data-scale"));
    expect(parseFloat(char.style.top)).toBe(parseFloat((desk as unknown as HTMLElement).style.top) - 5 * scale);
    // The character comes before the desk in the DOM, so the desk is drawn over its body.
    expect(char.compareDocumentPosition(desk) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });
});
