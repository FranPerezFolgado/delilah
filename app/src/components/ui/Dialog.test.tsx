// @vitest-environment jsdom
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Dialog } from "./Dialog";
import { ConfirmDialog } from "./ConfirmDialog";

afterEach(cleanup);

describe("Dialog", () => {
  it("is a labelled modal dialog", () => {
    render(<Dialog title="Nueva tarea" onClose={() => {}}><input aria-label="a" /></Dialog>);
    const d = screen.getByRole("dialog");
    expect(d.getAttribute("aria-modal")).toBe("true");
    const id = d.getAttribute("aria-labelledby")!;
    expect(document.getElementById(id)!.textContent).toBe("Nueva tarea");
  });

  it("closes on Escape", () => {
    const onClose = vi.fn();
    render(<Dialog title="x" onClose={onClose}><button>a</button></Dialog>);
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("moves focus in, traps Tab and returns focus on close", () => {
    function Host() {
      const [open, setOpen] = useState(false);
      return (
        <>
          <button onClick={() => setOpen(true)}>open</button>
          {open && (
            <Dialog title="x" onClose={() => setOpen(false)} footer={<button>last</button>}>
              <button>first</button>
            </Dialog>
          )}
        </>
      );
    }
    render(<Host />);
    const opener = screen.getByText("open");
    opener.focus();
    fireEvent.click(opener);
    const dialog = screen.getByRole("dialog");
    expect(dialog.contains(document.activeElement)).toBe(true);
    const first = screen.getByText("first");
    const last = screen.getByText("last");
    last.focus();
    fireEvent.keyDown(dialog, { key: "Tab" });
    expect(document.activeElement).toBe(first);
    fireEvent.keyDown(dialog, { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(last);
    fireEvent.keyDown(dialog, { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.activeElement).toBe(opener);
  });

  it("renders the footer and applies the size", () => {
    render(<Dialog title="x" onClose={() => {}} footer={<span>pie</span>} size={{ w: 400, h: 300 }} />);
    expect(screen.getByText("pie").closest(".dialog-ft")).not.toBeNull();
    const d = screen.getByRole("dialog");
    expect(d.style.width).toBe("400px");
    expect(d.style.height).toBe("300px");
  });
});

describe("ConfirmDialog", () => {
  it("orders the footer Cancelar then the action", () => {
    const onConfirm = vi.fn();
    const onClose = vi.fn();
    render(<ConfirmDialog title="Fusionar #1" body="¿Seguro?" action="Fusionar" onConfirm={onConfirm} onClose={onClose} />);
    const btns = screen.getAllByRole("button");
    expect(btns.map((b) => b.textContent)).toEqual([expect.stringMatching(/^(Cancelar|Cancel)$/), "Fusionar"]);
    fireEvent.click(btns[1]);
    expect(onConfirm).toHaveBeenCalled();
    fireEvent.click(btns[0]);
    expect(onClose).toHaveBeenCalled();
  });

  it("danger styles the action", () => {
    render(<ConfirmDialog title="t" body="b" action="Eliminar" danger onConfirm={() => {}} onClose={() => {}} />);
    expect(screen.getByText("Eliminar").className).toContain("btn-danger");
  });

  it("blocked shows the note and disables the action", () => {
    const onConfirm = vi.fn();
    render(<ConfirmDialog title="t" body="b" action="Ir" blocked="No se puede" onConfirm={onConfirm} onClose={() => {}} />);
    expect(screen.getByText("No se puede")).toBeTruthy();
    const a = screen.getByText("Ir") as HTMLButtonElement;
    expect(a.disabled).toBe(true);
    fireEvent.click(a);
    expect(onConfirm).not.toHaveBeenCalled();
  });
});
