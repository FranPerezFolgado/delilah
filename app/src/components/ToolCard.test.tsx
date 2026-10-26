// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { api, type SubagentProgress, type ToolEntry } from "../api";
import { setLanguage } from "../i18n";
import { ToolCard } from "./ToolCard";

beforeEach(() => setLanguage("es"));

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const tool = (over: Partial<ToolEntry> = {}): ToolEntry => ({
  id: "t1",
  at: "",
  kind: "tool",
  tool: "Bash",
  tool_use_id: "toolu_1",
  summary: "resumen",
  input: {},
  result: "",
  is_error: false,
  truncated: false,
  ...over,
});

function setup(entry: ToolEntry, onOpenFile = vi.fn(), subagent?: SubagentProgress | null) {
  const r = render(<ToolCard entry={entry} sessionId="s1" onOpenFile={onOpenFile} subagent={subagent} />);
  return { ...r, onOpenFile };
}

const open = (r: ReturnType<typeof setup>, _summary?: string) => fireEvent.click(r.container.querySelector(".tool-head")!);

describe("ToolCard", () => {
  it("starts collapsed with its summary and expands on click", () => {
    const r = setup(tool({ summary: "ls -la", input: { command: "ls -la" }, result: "total 0" }));
    expect(r.container.querySelector(".tool-sum")?.textContent).toBe("ls -la");
    expect(r.queryByText("total 0")).toBeNull();
    open(r, "ls -la");
    expect(r.getByText("total 0")).toBeTruthy();
    open(r, "ls -la");
    expect(r.queryByText("total 0")).toBeNull();
  });

  it("is a group named by its summary", () => {
    const r = setup(tool({ summary: "ls -la", input: { command: "ls -la" } }));
    expect(r.getByRole("group", { name: "ls -la" })).toBeTruthy();
  });

  it("Bash shows the command and the output", () => {
    const r = setup(tool({ summary: "lista", input: { command: "ls /tmp" }, result: "a\nb" }));
    open(r, "lista");
    expect(r.getByText("ls /tmp")).toBeTruthy();
    expect(r.container.querySelector(".tool-out")?.textContent).toBe("a\nb");
    expect(r.container.querySelector(".tool-card.failed")).toBeNull();
  });

  it("Bash is failed when is_error", () => {
    const r = setup(tool({ summary: "falla", input: { command: "false" }, result: "boom", is_error: true }));
    expect(r.container.querySelector(".tool-card.failed")).toBeTruthy();
    open(r, "falla");
    expect(r.getByText("Falló")).toBeTruthy();
  });

  it("Edit shows the path and the diff, and opens the file", () => {
    const diff = [
      { kind: "hunk" as const, old: null, new: null, text: "@@ -1 +1 @@" },
      { kind: "del" as const, old: 1, new: null, text: "viejo" },
      { kind: "add" as const, old: null, new: 1, text: "nuevo" },
    ];
    const r = setup(tool({ tool: "Edit", summary: "src/a.ts +1 −1", path: "src/a.ts", input: { file_path: "src/a.ts" }, diff }));
    open(r, "src/a.ts +1 −1");
    expect(r.container.querySelector(".dl.add")?.textContent).toContain("nuevo");
    expect(r.container.querySelector(".dl.del")?.textContent).toContain("viejo");
    fireEvent.click(r.getByText("Abrir en el editor"));
    expect(r.onOpenFile).toHaveBeenCalledWith("src/a.ts");
  });

  it("offers no editor button for a path outside the workspace (absolute)", () => {
    const r = setup(tool({ tool: "Edit", summary: "x", path: "/etc/hosts", input: { file_path: "/etc/hosts" } }));
    open(r);
    expect(r.queryByText("Abrir en el editor")).toBeNull();
  });

  it("Write shows the path and the open button", () => {
    const r = setup(tool({ tool: "Write", summary: "n.md +1 −0", path: "n.md", input: { file_path: "n.md" }, diff: [{ kind: "add", old: null, new: 1, text: "hola" }] }));
    open(r, "n.md +1 −0");
    expect(r.getAllByText("n.md").length).toBeGreaterThan(0);
    fireEvent.click(r.getByText("Abrir en el editor"));
    expect(r.onOpenFile).toHaveBeenCalledWith("n.md");
  });

  it("Read shows the path and the range", () => {
    const r = setup(tool({ tool: "Read", summary: "leer", input: { file_path: "src/b.ts", offset: 10, limit: 5 }, result: "x" }));
    open(r, "leer");
    expect(r.getByText("src/b.ts")).toBeTruthy();
    expect(r.getByText("10-14")).toBeTruthy();
  });

  it("a tool call without an input still renders", () => {
    for (const name of ["Bash", "Edit", "Read", "Grep", "TodoWrite", "Mystery"]) {
      const r = setup(tool({ tool: name, summary: `sin ${name}`, input: null as unknown as Record<string, unknown>, result: "x" }));
      open(r, `sin ${name}`);
      r.unmount();
    }
  });

  it("Grep and Glob show the pattern and the count", () => {
    const g = setup(tool({ tool: "Grep", summary: "buscar", input: { pattern: "foo.*bar" }, result: "a.ts\nb.ts\n" }));
    open(g, "buscar");
    expect(g.getByText("foo.*bar")).toBeTruthy();
    expect(g.getByText("2 resultados")).toBeTruthy();
    cleanup();
    const l = setup(tool({ tool: "Glob", summary: "globear", input: { pattern: "**/*.rs" }, result: "x.rs" }));
    open(l, "globear");
    expect(l.getByText("**/*.rs")).toBeTruthy();
    expect(l.getByText("1 resultado")).toBeTruthy();
  });

  it("TodoWrite shows the items", () => {
    const todos = [
      { content: "Escribir tests", status: "completed" },
      { content: "Implementar", status: "pending" },
    ];
    const r = setup(tool({ tool: "TodoWrite", summary: "2 tareas", input: { todos } }));
    open(r, "2 tareas");
    expect(r.getByText("Escribir tests")).toBeTruthy();
    expect(r.getByText("Implementar")).toBeTruthy();
  });

  it("Task shows the prompt and the result", () => {
    const r = setup(tool({ tool: "Task", summary: "investigar", input: { prompt: "Mira el repo" }, result: "Hecho" }));
    open(r, "investigar");
    expect(r.getByText("Mira el repo")).toBeTruthy();
    expect(r.getByText("Hecho")).toBeTruthy();
  });

  it("an unknown tool shows a generic card", () => {
    const r = setup(tool({ tool: "Frobnicate", summary: "Frobnicate {}", input: { a: 1 }, result: "ok!" }));
    open(r, "Frobnicate {}");
    expect(r.getByText("Frobnicate")).toBeTruthy();
    expect(r.container.textContent).toContain('"a": 1');
    expect(r.getByText("ok!")).toBeTruthy();
  });

  it("renders the output as text, not HTML", () => {
    const r = setup(tool({ summary: "x", result: "<b>negrita</b>" }));
    open(r, "x");
    expect(r.container.querySelector("b")).toBeNull();
    expect(r.getByText("<b>negrita</b>")).toBeTruthy();
  });

  it("truncated: Mostrar todo fetches and shows the full text", async () => {
    const spy = vi.spyOn(api, "toolDetail").mockResolvedValue({ input: { command: "cat big" }, result: "COMPLETO", is_error: false, truncated: false });
    const r = setup(tool({ summary: "grande", input: { command: "cat big" }, result: "corto", truncated: true }));
    open(r, "grande");
    fireEvent.click(r.getByText("Mostrar todo"));
    await waitFor(() => expect(r.getByText("COMPLETO")).toBeTruthy());
    expect(spy).toHaveBeenCalledWith("s1", "toolu_1");
    expect(r.queryByText("Mostrar todo")).toBeNull();
  });

  it("a pending tool shows a running state", () => {
    const r = setup(tool({ summary: "corriendo", input: { command: "sleep 9" }, result: null }));
    expect(r.container.querySelector(".tool-card.running")).toBeTruthy();
    open(r, "corriendo");
    expect(r.getByText("En curso…")).toBeTruthy();
  });
});

describe("ToolCard sub-agent progress", () => {
  const step = (n: number) => ({ tool: "Read", summary: `paso ${n}`, at: "" });
  const sub = (over: Partial<SubagentProgress> = {}): SubagentProgress => ({
    tool_use_id: "toolu_1",
    description: "d",
    state: "running",
    started_at: new Date(Date.now() - 5000).toISOString(),
    ended_at: null,
    steps: 3,
    recent: [step(1), step(2), step(3)],
    ...over,
  });
  const agent = (over: Partial<ToolEntry> = {}) => tool({ tool: "Agent", summary: "Agente", input: { prompt: "haz" }, result: null, ...over });

  it("shows state, clock, step count and latest step while running", () => {
    const r = setup(agent(), vi.fn(), sub());
    const head = r.container.querySelector(".tool-head")!.textContent!;
    expect(head).toContain("en curso");
    expect(head).toMatch(/\d+ s/);
    expect(head).toContain("3 pasos");
    expect(head).toContain("paso 3");
    expect(head).not.toContain("paso 2");
  });

  it("uses the singular for one step", () => {
    const r = setup(agent(), vi.fn(), sub({ steps: 1, recent: [step(1)] }));
    expect(r.container.querySelector(".tool-head")!.textContent).toContain("1 paso");
    expect(r.container.querySelector(".tool-head")!.textContent).not.toContain("1 pasos");
  });

  it("lists recent steps expanded, with the count of earlier ones", () => {
    const r = setup(agent(), vi.fn(), sub({ steps: 5 }));
    open(r);
    expect(r.container.querySelectorAll(".tool-steps li").length).toBe(3);
    expect(r.getByText("paso 1")).toBeTruthy();
    expect(r.getByText("(+2 anteriores)")).toBeTruthy();
  });

  it("has no earlier-steps note when all steps are listed", () => {
    const r = setup(agent(), vi.fn(), sub());
    open(r);
    expect(r.queryByText(/anteriores/)).toBeNull();
  });

  it("shows done with steps and minutes, failed and stopped", () => {
    const t0 = Date.parse("2026-01-01T00:00:00Z");
    const ended = new Date(t0 + 180000).toISOString();
    const base = { started_at: new Date(t0).toISOString(), ended_at: ended, steps: 4 };
    let r = setup(agent({ result: "ok" }), vi.fn(), sub({ ...base, state: "completed" }));
    expect(r.container.querySelector(".tool-head")!.textContent).toContain("Hecho · 4 pasos · 3 min");
    cleanup();
    r = setup(agent({ result: "x" }), vi.fn(), sub({ ...base, state: "failed" }));
    expect(r.container.querySelector(".tool-head")!.textContent).toContain("Fallido");
    cleanup();
    r = setup(agent({ result: "x" }), vi.fn(), sub({ ...base, state: "stopped" }));
    expect(r.container.querySelector(".tool-head")!.textContent).toContain("Detenido");
    cleanup();
    r = setup(agent({ result: "x" }), vi.fn(), sub({ ...base, state: "unknown" }));
    expect(r.container.querySelector(".tool-head")!.textContent).toContain("4 pasos");
  });

  it("renders as before without the prop", () => {
    const r = setup(agent());
    expect(r.container.querySelector(".tool-head")!.textContent).not.toContain("pasos");
    open(r);
    expect(r.container.querySelector(".tool-steps")).toBeNull();
  });
});
