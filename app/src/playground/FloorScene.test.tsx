// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render } from "@testing-library/react";
import type { DeskSeat } from "./useOffice";
import { DeskArt, SceneArt, glowOf, seatedProps } from "./FloorScene";

afterEach(cleanup);

const seat = (over: Partial<DeskSeat> = {}): DeskSeat => ({ slot: 0, session: "s", state: "working", action: null, lookKey: "p", ...over });

describe("FloorScene", () => {
  it("picks the seated sprite by state", () => {
    expect(seatedProps(seat({ state: "paused" }))).toEqual({ sprite: "seated_sleeping" });
    expect(seatedProps(seat({ state: "working", action: { kind: "edit" } as DeskSeat["action"] }))).toEqual({ animation: "typing" });
    expect(seatedProps(seat({ state: "waiting" }))).toEqual({ sprite: "seated" });
  });

  it("has no glow for a paused desk", () => {
    expect(glowOf(seat({ state: "paused", action: { kind: "edit" } as DeskSeat["action"] }))).toBeNull();
  });

  it("marks a waiting desk with the bar and ?, and a paused one with zzz", () => {
    const w = render(<DeskArt seat={seat({ state: "waiting" })} scale={1}><i /></DeskArt>);
    expect(w.container.querySelector(".fl-bar")).toBeTruthy();
    expect(w.container.querySelector(".fl-bubble--wait")?.textContent).toBe("?");
    cleanup();
    const p = render(<DeskArt seat={seat({ state: "paused" })} scale={1}><i /></DeskArt>);
    expect(p.container.querySelector(".fl-bubble--zzz")?.textContent).toBe("zzz");
  });

  it("draws wall decor and furniture", () => {
    const { container } = render(<SceneArt scale={1} pods={1} />);
    expect(container.querySelector("svg.fl-deco")).toBeTruthy();
    expect(container.querySelectorAll(".fl-abs").length).toBeGreaterThan(5);
  });
});
