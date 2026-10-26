// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { HistoryEntry } from "../api";
import { t } from "../i18n";
import { FlowView } from "./FlowView";

const at = (s: number) => `2026-01-01T10:00:${String(s).padStart(2, "0")}Z`;
const act = (id: string, tool: string, target: string, kind: "read" | "edit") => ({ id, kind, tool, target, background: false, since: at(1) });
const entries: HistoryEntry[] = [
  { seq: 1, at: at(1), type: "tool_started", agent: null, action: act("a", "Read", "a.ts", "read") },
  { seq: 2, at: at(3), type: "tool_ended", agent: null, id: "a", failed: false },
  { seq: 3, at: at(4), type: "tool_started", agent: null, action: act("b", "Edit", "b.ts", "edit") },
  { seq: 4, at: at(6), type: "tool_ended", agent: null, id: "b", failed: true },
  { seq: 5, at: at(7), type: "tool_started", agent: null, action: act("c", "Bash", "ls", "read") },
] as HistoryEntry[];

const hook = vi.hoisted(() => ({ value: { startedAt: "x", sessions: [] as unknown[], stale: false } }));
vi.mock("./useActivityHistory", () => ({ useActivityHistory: () => hook.value }));

const setHistory = (over: Partial<typeof hook.value> = {}, es: HistoryEntry[] = entries, dropped = false) => {
  hook.value = { startedAt: at(0), sessions: [{ session: "x", live: true, dropped, entries: es }], stale: false, ...over };
};
const vb = (c: HTMLElement) => c.querySelector("svg")!.getAttribute("viewBox")!.split(" ").map(Number);

afterEach(cleanup);

describe("FlowView", () => {
  it("draws a node per agent and tool call with state classes", () => {
    setHistory();
    render(<FlowView owner="T-1" />);
    expect(screen.getAllByRole("button", { name: /^Read,/ })).toHaveLength(1);
    expect(screen.getByRole("button", { name: /^Edit,/ }).getAttribute("data-state")).toBe("failed");
    expect(screen.getByRole("button", { name: /^Bash,/ }).getAttribute("data-state")).toBe("running");
    expect(screen.getByRole("button", { name: /^Read,/ }).getAttribute("data-state")).toBe("done");
  });

  it("selects a node by click or Enter and shows its details", () => {
    setHistory();
    render(<FlowView owner="T-1" />);
    fireEvent.click(screen.getByRole("button", { name: /^Bash,/ }));
    expect(screen.getByRole("button", { name: /^Bash,/ }).getAttribute("aria-pressed")).toBe("true");
    expect(screen.getByText(t("en curso"), { selector: ".fv-details *" })).toBeTruthy();
    fireEvent.keyDown(screen.getByRole("button", { name: /^Edit,/ }), { key: "Enter" });
    expect(screen.getByRole("button", { name: /^Edit,/ }).getAttribute("aria-pressed")).toBe("true");
    expect(screen.queryByText(t("en curso"), { selector: ".fv-details *" })).toBeNull();
    expect(screen.getByText("b.ts", { selector: ".fv-details *" })).toBeTruthy();
  });

  it("syncs selection with the timeline both ways", () => {
    setHistory();
    render(<FlowView owner="T-1" />);
    fireEvent.click(screen.getByRole("button", { name: /^Bash,/ }));
    const sel = screen.getAllByRole("option").filter((o) => o.getAttribute("aria-selected") === "true");
    expect(sel).toHaveLength(1);
    expect(sel[0].textContent).toContain("Bash");
    fireEvent.click(screen.getAllByRole("option").find((o) => o.textContent!.includes("Read"))!);
    expect(screen.getByRole("button", { name: /^Read,/ }).getAttribute("aria-pressed")).toBe("true");
  });

  it("zoom buttons and fit change the viewBox", () => {
    setHistory();
    const { container } = render(<FlowView owner="T-1" />);
    const fit = vb(container);
    fireEvent.click(screen.getByRole("button", { name: t("Acercar") }));
    const zin = vb(container);
    expect(zin[2]).toBeLessThan(fit[2]);
    fireEvent.click(screen.getByRole("button", { name: t("Alejar") }));
    fireEvent.click(screen.getByRole("button", { name: t("Alejar") }));
    expect(vb(container)[2]).toBeGreaterThan(fit[2]);
    fireEvent.click(screen.getByRole("button", { name: t("Ajustar") }));
    expect(vb(container)).toEqual(fit);
  });

  it("wheel zooms and drag pans", () => {
    setHistory();
    const { container } = render(<FlowView owner="T-1" />);
    const svg = container.querySelector("svg")!;
    const fit = vb(container);
    fireEvent.wheel(svg, { deltaY: -100, clientX: 0, clientY: 0 });
    expect(vb(container)[2]).toBeLessThan(fit[2]);
    const before = vb(container);
    fireEvent.pointerDown(svg, { clientX: 10, clientY: 10, pointerId: 1, button: 0 });
    fireEvent.pointerMove(svg, { clientX: 30, clientY: 20, pointerId: 1 });
    fireEvent.pointerUp(svg, { pointerId: 1 });
    expect(vb(container)[0]).not.toBe(before[0]);
  });

  it("notes empty and dropped history", () => {
    setHistory({ sessions: [] });
    const { unmount } = render(<FlowView owner="T-1" />);
    expect(screen.getByText(t("Sin actividad registrada todavía"))).toBeTruthy();
    unmount();
    setHistory({}, entries, true);
    render(<FlowView owner="T-1" />);
    expect(screen.getByText(t("Se perdió parte del historial; el gráfico puede estar incompleto"))).toBeTruthy();
  });

  it("dims when stale and renders headerExtra", () => {
    setHistory({ stale: true });
    const { container } = render(<FlowView owner="T-1" headerExtra={<i data-testid="x" />} />);
    expect(container.querySelector(".fv--stale")).toBeTruthy();
    expect(screen.getByTestId("x")).toBeTruthy();
  });
});
