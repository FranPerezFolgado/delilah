// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/react";
import type { BackgroundInfo, ConversationEntry } from "../api";
import { setLanguage } from "../i18n";
import { BackgroundDoneItem } from "./BackgroundDoneItem";

type Marker = Extract<ConversationEntry, { kind: "marker" }>;
const mk = (background?: Partial<BackgroundInfo>, detail?: string): Marker => ({
  id: "m1", at: "2025-01-01T00:00:00Z", kind: "marker", marker: "background-done", detail,
  background: background && ({ source: "notification", kind: "command", ...background } as BackgroundInfo),
});
const row = (e: Marker) => render(<BackgroundDoneItem entry={e} complete={true} />).container.firstElementChild!;

beforeEach(() => setLanguage("es"));
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("BackgroundDoneItem", () => {
  it("renders an agent with metrics", () => {
    const r = row(mk({ kind: "agent", status: "completed", label: "WP-07 update status.md", duration_ms: 89000, tool_uses: 2, tokens: 12300 }));
    expect(r.textContent).toContain("Subagente terminado · WP-07 update status.md · 1 min 29 s · 2 herramientas · 12.3K tokens");
  });
  it("uses the singular for one tool", () => {
    expect(row(mk({ kind: "agent", status: "completed", tool_uses: 1 })).textContent).toContain("1 herramienta");
    expect(row(mk({ kind: "agent", status: "completed", tool_uses: 1 })).textContent).not.toContain("herramientas");
  });
  it("renders a command with its exit code", () => {
    const r = row(mk({ kind: "command", status: "completed", exit_code: 0, label: "Sleep 25 seconds in background" }));
    expect(r.textContent).toContain("Comando terminado · Sleep 25 seconds in background · código 0");
    expect(r.classList.contains("warn")).toBe(false);
  });
  it("flags failed and stopped as warnings", () => {
    const f = row(mk({ status: "failed" }));
    expect(f.textContent).toContain("falló");
    expect(f.classList.contains("warn")).toBe(true);
    const s = row(mk({ kind: "task", status: "stopped" }));
    expect(s.textContent).toContain("detenida");
    expect(s.classList.contains("warn")).toBe(true);
  });
  it("shows an unknown status raw and neutral", () => {
    const r = row(mk({ status: "weird" }));
    expect(r.textContent).toContain("weird");
    expect(r.classList.contains("warn")).toBe(false);
  });
  it("leaves out missing metrics", () => {
    const r = row(mk({ kind: "agent", status: "completed", label: "X" }));
    expect(r.textContent).toContain("Subagente terminado · X");
    expect(r.textContent).not.toMatch(/0|· ·|·\s*$/);
  });
  it("falls back to the old line without background", () => {
    expect(row(mk(undefined, "some detail")).textContent).toBe("Tarea en segundo plano terminada · some detail");
  });
  it("never shows raw task tags", () => {
    expect(row(mk({ status: "completed", label: "x" })).textContent).not.toContain("<task-");
    expect(row(mk(undefined, "d")).textContent).not.toContain("<task-");
  });
  const agent = (o: Partial<BackgroundInfo>) => mk({ kind: "agent", status: "completed", ...o });
  it("reads as finished when the status is missing or unknown", () => {
    const a = row(mk({ kind: "agent", label: "X" }));
    expect(a.textContent).toContain("Subagente terminado · X");
    expect(a.classList.contains("warn")).toBe(false);
    expect(row(mk({ kind: "task" })).textContent).toBe("•Tarea en segundo plano terminada");
    const c = row(mk({ kind: "command", status: "weird" }));
    expect(c.textContent).toContain("Comando terminado (weird)");
    expect(c.classList.contains("warn")).toBe(false);
  });
  it("expands and collapses the report as markdown", () => {
    const { container, getByRole } = render(<BackgroundDoneItem entry={agent({ report: "# Title\n\n- one\n- two" })} complete={true} />);
    const btn = getByRole("button");
    expect(btn.getAttribute("aria-expanded")).toBe("false");
    expect(container.querySelector("h1")).toBeNull();
    fireEvent.click(btn);
    expect(container.querySelector("h1")?.textContent).toBe("Title");
    expect(container.querySelectorAll("li")).toHaveLength(2);
    const first = container.querySelector(".bg-done-report")!.firstElementChild!;
    expect(first.tagName).toBe("BUTTON");
    expect(first.getAttribute("aria-expanded")).toBe("true");
    fireEvent.click(first);
    expect(container.querySelector("h1")).toBeNull();
  });
  it("notes a truncated report", () => {
    const { container, getByRole } = render(<BackgroundDoneItem entry={agent({ report: "x", report_truncated: true })} complete={true} />);
    fireEvent.click(getByRole("button"));
    expect(container.textContent).toContain("(informe recortado)");
  });
  it("says loading or unavailable when the report is in a hand-back", () => {
    const e = agent({ report_in_handback: true });
    const a = render(<BackgroundDoneItem entry={e} complete={false} />);
    fireEvent.click(a.getByRole("button"));
    expect(a.container.textContent).toContain("Cargando informe…");
    cleanup();
    const b = render(<BackgroundDoneItem entry={e} complete={true} />);
    fireEvent.click(b.getByRole("button"));
    expect(b.container.textContent).toContain("Informe no disponible");
  });
  it("has no disclosure for a command", () => {
    expect(render(<BackgroundDoneItem entry={mk({ kind: "command", status: "completed" })} complete={true} />).queryByRole("button")).toBeNull();
  });
  it("renders script text as text", () => {
    const { container, getByRole } = render(<BackgroundDoneItem entry={agent({ report: "<script>alert(1)</script>" })} complete={true} />);
    fireEvent.click(getByRole("button"));
    expect(container.querySelector("script")).toBeNull();
    expect(container.textContent).toContain("<script>");
  });
});
