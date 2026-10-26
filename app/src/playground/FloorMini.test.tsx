// @vitest-environment jsdom
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { DeskSeat, FloorModel } from "./useOffice";
import { setLanguage } from "../i18n";
import { FloorMini } from "./FloorMini";

beforeAll(() => setLanguage("es"));

const lead = (i: number, over: Partial<DeskSeat> = {}): DeskSeat => ({ slot: i, session: `s${i}`, state: "working", action: null, lookKey: "p", ...over });
const helper = (i: number): DeskSeat => ({ slot: i, helperId: `h${i}`, state: "working", action: null, lookKey: "implementar" });
const floor = (over: Partial<FloorModel> = {}): FloorModel => ({
  owner: "LIBRE-3",
  title: "Un título muy largo de tarea",
  phase: "implement",
  project: "p",
  leads: [lead(0), lead(1, { state: "waiting" })],
  helpers: [],
  leaving: [],
  waiting: 1,
  ...over,
});

afterEach(cleanup);

describe("FloorMini", () => {
  it("shows key, title and phase, with the full title on hover", () => {
    render(<FloorMini floor={floor()} onOpen={() => {}} />);
    const b = screen.getByRole("button");
    expect(b.textContent).toContain("LIBRE-3");
    expect(b.textContent).toContain("Implementación");
    expect(b.getAttribute("title")).toBe("Un título muy largo de tarea");
  });

  it("has a descriptive accessible name", () => {
    render(<FloorMini floor={floor()} onOpen={() => {}} />);
    expect(screen.getByRole("button", { name: "LIBRE-3, Implementación, 2 agentes, 1 esperando" })).toBeTruthy();
  });

  it("shows at most 6 characters and +N", () => {
    const f = floor({ leads: [lead(0)], helpers: [1, 2, 3, 4, 5, 6, 7].map(helper), waiting: 0 });
    const { container } = render(<FloorMini floor={f} onOpen={() => {}} />);
    expect(container.querySelectorAll(".fm-mini")).toHaveLength(6);
    expect(container.querySelector(".fm-more")?.textContent).toBe("+2");
  });

  it("marks a waiting floor with the pill and highlight", () => {
    const { container } = render(<FloorMini floor={floor()} onOpen={() => {}} />);
    expect(container.querySelector(".pill.c-wait")?.textContent).toBe("1 esperando");
    expect(container.querySelector(".fm.fm-wait")).toBeTruthy();
  });

  it("has no pill when nobody waits", () => {
    const { container } = render(<FloorMini floor={floor({ waiting: 0, leads: [lead(0)] })} onOpen={() => {}} />);
    expect(container.querySelector(".pill")).toBeNull();
    expect(container.querySelector(".fm-wait")).toBeNull();
  });

  it("click opens the owner", () => {
    const onOpen = vi.fn();
    render(<FloorMini floor={floor()} onOpen={onOpen} />);
    fireEvent.click(screen.getByRole("button"));
    expect(onOpen).toHaveBeenCalledWith("LIBRE-3");
  });
});
