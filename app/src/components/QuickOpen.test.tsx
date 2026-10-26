// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { api } from "../api";
import { setLanguage } from "../i18n";
import { QuickOpen, matchedAt } from "./QuickOpen";

beforeEach(() => {
  setLanguage("es");
  Element.prototype.scrollIntoView = () => {};
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const FILES = ["src/main.rs", "src/lib/parser.rs", "README.md"];

async function open(onOpen = vi.fn(), onClose = vi.fn()) {
  vi.spyOn(api, "files").mockResolvedValue(FILES);
  const r = render(<QuickOpen taskKey="T-1" onOpen={onOpen} onClose={onClose} />);
  await waitFor(() => expect(r.container.ownerDocument.querySelectorAll(".qo-row").length).toBe(3));
  return { ...r, input: r.getByLabelText("Nombre del archivo") as HTMLInputElement, onOpen, onClose };
}

describe("QuickOpen", () => {
  it("is a dialog with the file count placeholder and the key hints", async () => {
    const { getByRole, input } = await open();
    expect(getByRole("dialog").textContent).toContain("Abrir archivo");
    expect(input.placeholder).toBe("Buscar 3 archivos…");
    const hints = document.querySelector(".qo-hints")!.textContent!;
    for (const h of ["↑↓", "↵", "Esc"]) expect(hints).toContain(h);
  });

  it("bolds the matched characters", async () => {
    const { input } = await open();
    fireEvent.change(input, { target: { value: "pars" } });
    const row = document.querySelector(".qo-row")!;
    expect(row.textContent).toContain("parser.rs");
    expect([...row.querySelectorAll("b")].map((b) => b.textContent).join("")).toBe("pars");
  });

  it("matchedAt covers a name prefix, a scattered fit and an empty query", () => {
    expect([...matchedAt("ma", "src/main.rs")]).toEqual([4, 5]);
    expect([...matchedAt("mrs", "src/main.rs")]).toEqual([4, 9, 10]);
    expect(matchedAt("", "x").size).toBe(0);
  });

  it("↓ and ↵ open the second file and close", async () => {
    const { input, onOpen, onClose } = await open();
    fireEvent.keyDown(input, { key: "ArrowDown" });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(onOpen).toHaveBeenCalledWith("src/lib/parser.rs");
    expect(onClose).toHaveBeenCalled();
  });

  it("Esc closes", async () => {
    const { input, onClose } = await open();
    fireEvent.keyDown(input, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("says when nothing matches", async () => {
    const { input, getByText } = await open();
    fireEvent.change(input, { target: { value: "zzzz" } });
    expect(getByText("Ningún archivo coincide.")).toBeTruthy();
  });
});
