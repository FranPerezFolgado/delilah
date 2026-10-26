// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/react";
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
});
