// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { api, type ToolEntry } from "../api";
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

function setup(entry: ToolEntry, onOpenFile = vi.fn()) {
  const r = render(<ToolCard entry={entry} sessionId="s1" onOpenFile={onOpenFile} />);
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
    expect(r.getByText("10-15")).toBeTruthy();
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

  it("truncated: Ver todo fetches and shows the full text", async () => {
    const spy = vi.spyOn(api, "toolDetail").mockResolvedValue({ input: { command: "cat big" }, result: "COMPLETO", is_error: false, truncated: false });
    const r = setup(tool({ summary: "grande", input: { command: "cat big" }, result: "corto", truncated: true }));
    open(r, "grande");
    fireEvent.click(r.getByText("Ver todo"));
    await waitFor(() => expect(r.getByText("COMPLETO")).toBeTruthy());
    expect(spy).toHaveBeenCalledWith("s1", "toolu_1");
    expect(r.queryByText("Ver todo")).toBeNull();
  });

  it("a pending tool shows a running state", () => {
    const r = setup(tool({ summary: "corriendo", input: { command: "sleep 9" }, result: null }));
    expect(r.container.querySelector(".tool-card.running")).toBeTruthy();
    open(r, "corriendo");
    expect(r.getByText("En curso…")).toBeTruthy();
  });
});
