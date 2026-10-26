// @vitest-environment jsdom
import { act } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { ToastHost, useToast, clearToasts, pushToast, dismissToast, dismissToastByKey } from "./Toast";

let toast: ReturnType<typeof useToast>;
function Probe() {
  toast = useToast();
  return null;
}
function mount() {
  render(<><Probe /><ToastHost /></>);
}

beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  act(() => clearToasts());
  cleanup();
  vi.useRealTimers();
});

describe("Toast", () => {
  it("shows a status toast and auto-dismisses info after the default 6 s", () => {
    mount();
    act(() => void toast({ text: "Hecho" }));
    expect(screen.getByText("Hecho")).not.toBeNull();
    act(() => void vi.advanceTimersByTime(5900));
    expect(screen.queryByText("Hecho")).not.toBeNull();
    act(() => void vi.advanceTimersByTime(200));
    expect(screen.queryByText("Hecho")).toBeNull();
  });

  it("honours ttl", () => {
    mount();
    act(() => void toast({ text: "rápido", ttl: 1000 }));
    act(() => void vi.advanceTimersByTime(1100));
    expect(screen.queryByText("rápido")).toBeNull();
  });

  it("errors and 'te espera' persist", () => {
    mount();
    act(() => {
      toast({ text: "fallo", kind: "error" });
      toast({ text: "espera", kind: "wait" });
    });
    act(() => void vi.advanceTimersByTime(60000));
    expect(screen.queryByText("fallo")).not.toBeNull();
    expect(screen.queryByText("espera")).not.toBeNull();
  });

  it("keeps at most 3, dropping the oldest", () => {
    mount();
    act(() => {
      for (const n of ["a", "b", "c", "d"]) toast({ text: n, kind: "error" });
    });
        expect(screen.queryByText("a")).toBeNull();
    expect(screen.queryByText("d")).not.toBeNull();
  });

  it("runs the action and dismisses", () => {
    mount();
    const run = vi.fn();
    act(() => void toast({ text: "Archivada", action: { label: "Deshacer", run } }));
    fireEvent.click(screen.getByText("Deshacer"));
    expect(run).toHaveBeenCalled();
    expect(screen.queryByText("Archivada")).toBeNull();
  });

  it("can be dismissed by hand", () => {
    mount();
    act(() => void toast({ text: "x", kind: "error" }));
    fireEvent.click(screen.getByRole("button", { name: /Cerrar|Close/ }));
    expect(screen.queryByText("x")).toBeNull();
  });

  it("a keyed push updates in place and returns the same id", () => {
    mount();
    let a = 0, b = 0;
    act(() => void (a = toast({ key: "k", text: "uno", kind: "error" })));
    act(() => void (b = toast({ key: "k", text: "dos", kind: "error", action: { label: "Ir", run: () => {} } })));
    expect(b).toBe(a);
    expect(screen.queryByText("uno")).toBeNull();
    expect(screen.getByText("dos")).not.toBeNull();
    expect(screen.getByText("Ir")).not.toBeNull();
    expect(screen.getAllByText("×").length).toBe(1);
  });

  it("dismissToastByKey removes without onDismiss", () => {
    mount();
    const onDismiss = vi.fn();
    act(() => void toast({ key: "k", text: "kk", onDismiss }));
    act(() => dismissToastByKey("k"));
    expect(screen.queryByText("kk")).toBeNull();
    expect(onDismiss).not.toHaveBeenCalled();
  });

  it("only the x button calls onDismiss", () => {
    mount();
    const onDismiss = vi.fn();
    act(() => void toast({ text: "x", kind: "error", onDismiss }));
    fireEvent.click(screen.getByRole("button", { name: /Cerrar|Close/ }));
    expect(onDismiss).toHaveBeenCalledTimes(1);
    const o2 = vi.fn();
    let id = 0;
    act(() => void (id = toast({ text: "y", ttl: 1000, onDismiss: o2 })));
    act(() => void vi.advanceTimersByTime(1100));
    act(() => void (id = toast({ text: "z", kind: "error", onDismiss: o2 })));
    act(() => dismissToast(id));
    expect(o2).not.toHaveBeenCalled();
  });

  it("info timers live in the store, without a host", () => {
    pushToast({ text: "solo", ttl: 1000 });
    pushToast({ text: "err", kind: "error" });
    pushToast({ text: "kw", key: "w" });
    vi.advanceTimersByTime(60000);
    mount();
    expect(screen.queryByText("solo")).toBeNull();
    expect(screen.queryByText("err")).not.toBeNull();
    expect(screen.queryByText("kw")).not.toBeNull();
  });

  it("evicts plain info first, then oldest non-keyed, never keyed", () => {
    mount();
    act(() => {
      toast({ text: "k", key: "k", kind: "error" });
      toast({ text: "act", action: { label: "A", run: () => {} } });
      toast({ text: "info" });
      toast({ text: "n1", kind: "error" });
    });
    expect(screen.queryByText("info")).toBeNull();
    expect(screen.queryByText("act")).not.toBeNull();
    act(() => void toast({ text: "n2", kind: "error" }));
    expect(screen.queryByText("act")).toBeNull();
    expect(screen.queryByText("k")).not.toBeNull();
    expect(screen.queryByText("n1")).not.toBeNull();
    expect(screen.queryByText("n2")).not.toBeNull();
  });

  it("merges repeats into one toast with a count, moved to newest", () => {
    mount();
    act(() => {
      toast({ text: "No se pudo conectar", kind: "error" });
      toast({ text: "otro", kind: "error" });
      toast({ text: "No se pudo conectar", kind: "error" });
      toast({ text: "No se pudo conectar", kind: "error" });
    });
    expect(screen.getAllByText("No se pudo conectar").length).toBe(1);
    expect(screen.getByText("×3")).not.toBeNull();
    const texts = Array.from(document.querySelectorAll(".toast-text")).map((e) => e.textContent);
    expect(texts).toEqual(["otro", "No se pudo conectar"]);
  });

  it("a merged info toast restarts its full ttl", () => {
    mount();
    act(() => void toast({ text: "hola" }));
    act(() => void vi.advanceTimersByTime(5000));
    act(() => void toast({ text: "hola" }));
    act(() => void vi.advanceTimersByTime(5000));
    expect(screen.queryByText("hola")).not.toBeNull();
    expect(screen.getByText("×2")).not.toBeNull();
    act(() => void vi.advanceTimersByTime(1100));
    expect(screen.queryByText("hola")).toBeNull();
  });

  it("does not merge different kinds", () => {
    mount();
    act(() => {
      toast({ text: "igual" });
      toast({ text: "igual", kind: "error" });
    });
    expect(screen.getAllByText("igual").length).toBe(2);
    expect(screen.queryByText("×2")).toBeNull();
  });

  it("does not merge toasts with an action", () => {
    mount();
    act(() => {
      toast({ text: "Archivada", action: { label: "Deshacer", run: () => {} } });
      toast({ text: "Archivada", action: { label: "Deshacer", run: () => {} } });
    });
    expect(screen.getAllByText("Deshacer").length).toBe(2);
  });

  it("dismissToast with any repeat id removes the merged toast", () => {
    mount();
    const ids: number[] = [];
    act(() => {
      for (let n = 0; n < 3; n++) ids.push(toast({ text: "rep", kind: "error" }));
    });
    act(() => dismissToast(ids[1]));
    expect(screen.queryByText("rep")).toBeNull();
    act(() => void ids.push(toast({ text: "rep2", kind: "error" })));
    act(() => dismissToast(ids[2]));
    expect(screen.queryByText("rep2")).not.toBeNull();
  });

  it("caps the displayed count at 99+", () => {
    mount();
    act(() => {
      for (let n = 0; n < 120; n++) toast({ text: "spam", kind: "error" });
    });
    expect(screen.getByText("×99+")).not.toBeNull();
  });
});
