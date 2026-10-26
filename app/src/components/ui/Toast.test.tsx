// @vitest-environment jsdom
import { act } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { ToastHost, useToast, clearToasts, pushToast, dismissToast, dismissToastByKey } from "./Toast";

const MORE = /Mostrar 2 avisos más|Show 2 more notices/;
let toast: ReturnType<typeof useToast>;
function Probe() {
  toast = useToast();
  return null;
}
function mount() {
  render(<><Probe /><ToastHost /></>);
}
/** Text queries that ignore the screen-reader announcer. */
const gat = (m: string | RegExp) => screen.queryAllByText(m).filter((e) => !e.closest(".toast-sr"));
const qt = (m: string | RegExp) => gat(m)[0] ?? null;
const gt = (m: string | RegExp) => {
  const r = gat(m);
  if (r.length !== 1) throw new Error(`expected one match for ${m}, got ${r.length}`);
  return r[0];
};
/** Several toasts are collapsed to the newest; hover to read them all. */
function expand() {
  const host = document.querySelector(".toast-host");
  if (host) fireEvent.mouseEnter(host);
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
    expect(gt("Hecho")).not.toBeNull();
    act(() => void vi.advanceTimersByTime(5900));
    expect(qt("Hecho")).not.toBeNull();
    act(() => void vi.advanceTimersByTime(200));
    expect(qt("Hecho")).toBeNull();
  });

  it("honours ttl", () => {
    mount();
    act(() => void toast({ text: "rápido", ttl: 1000 }));
    act(() => void vi.advanceTimersByTime(1100));
    expect(qt("rápido")).toBeNull();
  });

  it("errors and 'te espera' persist", () => {
    mount();
    act(() => {
      toast({ text: "fallo", kind: "error" });
      toast({ text: "espera", kind: "wait" });
    });
    act(() => void vi.advanceTimersByTime(60000));
    expand();
    expect(qt("fallo")).not.toBeNull();
    expect(qt("espera")).not.toBeNull();
  });

  it("keeps at most 3, dropping the oldest", () => {
    mount();
    act(() => {
      for (const n of ["a", "b", "c", "d"]) toast({ text: n, kind: "error" });
    });
        expect(qt("a")).toBeNull();
    expect(qt("d")).not.toBeNull();
  });

  it("runs the action and dismisses", () => {
    mount();
    const run = vi.fn();
    act(() => void toast({ text: "Archivada", action: { label: "Deshacer", run } }));
    fireEvent.click(gt("Deshacer"));
    expect(run).toHaveBeenCalled();
    expect(qt("Archivada")).toBeNull();
  });

  it("can be dismissed by hand", () => {
    mount();
    act(() => void toast({ text: "x", kind: "error" }));
    fireEvent.click(screen.getByRole("button", { name: /Cerrar|Close/ }));
    expect(qt("x")).toBeNull();
  });

  it("a keyed push updates in place and returns the same id", () => {
    mount();
    let a = 0, b = 0;
    act(() => void (a = toast({ key: "k", text: "uno", kind: "error" })));
    act(() => void (b = toast({ key: "k", text: "dos", kind: "error", action: { label: "Ir", run: () => {} } })));
    expect(b).toBe(a);
    expect(qt("uno")).toBeNull();
    expect(gt("dos")).not.toBeNull();
    expect(gt("Ir")).not.toBeNull();
    expect(gat("×").length).toBe(1);
  });

  it("dismissToastByKey removes without onDismiss", () => {
    mount();
    const onDismiss = vi.fn();
    act(() => void toast({ key: "k", text: "kk", onDismiss }));
    act(() => dismissToastByKey("k"));
    expect(qt("kk")).toBeNull();
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
    expect(qt("solo")).toBeNull();
    expand();
    expect(qt("err")).not.toBeNull();
    expect(qt("kw")).not.toBeNull();
  });

  it("evicts plain info first, then oldest non-keyed, never keyed", () => {
    mount();
    act(() => {
      toast({ text: "k", key: "k", kind: "error" });
      toast({ text: "act", action: { label: "A", run: () => {} } });
      toast({ text: "info" });
      toast({ text: "n1", kind: "error" });
    });
    expand();
    expect(qt("info")).toBeNull();
    expect(qt("act")).not.toBeNull();
    act(() => void toast({ text: "n2", kind: "error" }));
    expand();
    expect(qt("act")).toBeNull();
    expect(qt("k")).not.toBeNull();
    expect(qt("n1")).not.toBeNull();
    expect(qt("n2")).not.toBeNull();
  });

  it("merges repeats into one toast with a count, moved to newest", () => {
    mount();
    act(() => {
      toast({ text: "No se pudo conectar", kind: "error" });
      toast({ text: "otro", kind: "error" });
      toast({ text: "No se pudo conectar", kind: "error" });
      toast({ text: "No se pudo conectar", kind: "error" });
    });
    expand();
    expect(gat("No se pudo conectar").length).toBe(1);
    expect(gt("×3")).not.toBeNull();
    const texts = Array.from(document.querySelectorAll(".toast-text")).map((e) => e.textContent);
    expect(texts).toEqual(["otro", "No se pudo conectar"]);
  });

  it("a merged info toast restarts its full ttl", () => {
    mount();
    act(() => void toast({ text: "hola" }));
    act(() => void vi.advanceTimersByTime(5000));
    act(() => void toast({ text: "hola" }));
    act(() => void vi.advanceTimersByTime(5000));
    expect(qt("hola")).not.toBeNull();
    expect(gt("×2")).not.toBeNull();
    act(() => void vi.advanceTimersByTime(1100));
    expect(qt("hola")).toBeNull();
  });

  it("does not merge different kinds", () => {
    mount();
    act(() => {
      toast({ text: "igual" });
      toast({ text: "igual", kind: "error" });
    });
    expand();
    expect(gat("igual").length).toBe(2);
    expect(qt("×2")).toBeNull();
  });

  it("does not merge toasts with an action", () => {
    mount();
    act(() => {
      toast({ text: "Archivada", action: { label: "Deshacer", run: () => {} } });
      toast({ text: "Archivada", action: { label: "Deshacer", run: () => {} } });
    });
    expand();
    expect(gat("Deshacer").length).toBe(2);
  });

  it("dismissToast with any repeat id removes the merged toast", () => {
    mount();
    const ids: number[] = [];
    act(() => {
      for (let n = 0; n < 3; n++) ids.push(toast({ text: "rep", kind: "error" }));
    });
    act(() => dismissToast(ids[1]));
    expect(qt("rep")).toBeNull();
    act(() => void ids.push(toast({ text: "rep2", kind: "error" })));
    act(() => dismissToast(ids[2]));
    expect(qt("rep2")).not.toBeNull();
  });

  it("caps the displayed count at 99+", () => {
    mount();
    act(() => {
      for (let n = 0; n < 120; n++) toast({ text: "spam", kind: "error" });
    });
    expect(gt("×99+")).not.toBeNull();
  });

  it("a keyed update replaces the item: omitted action and onDismiss do not survive", () => {
    mount();
    const onDismiss = vi.fn();
    act(() => void toast({ key: "k", text: "uno", kind: "error", action: { label: "Ir", run: () => {} }, onDismiss }));
    act(() => void toast({ key: "k", text: "dos", kind: "error" }));
    expect(qt("Ir")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: /Cerrar|Close/ }));
    expect(onDismiss).not.toHaveBeenCalled();
  });

  it("eviction never drops the toast just pushed", () => {
    mount();
    act(() => {
      toast({ text: "e", kind: "error" });
      toast({ text: "u", action: { label: "U", run: () => {} } });
      toast({ text: "w", kind: "wait" });
      toast({ text: "nuevo" });
    });
    expect(qt("nuevo")).not.toBeNull();
    expect(qt("e")).toBeNull();
  });

  describe("collapsed stack", () => {
    const three = () =>
      act(() => {
        toast({ text: "uno", kind: "error" });
        toast({ text: "dos", kind: "error" });
        toast({ text: "tres", kind: "error" });
      });
    const host = () => document.querySelector(".toast-host") as HTMLElement;

    it("one toast has no +N and no Cerrar todo", () => {
      mount();
      act(() => void toast({ text: "solo", kind: "error" }));
      expect(qt(/^\+/)).toBeNull();
      expect(qt(/Cerrar todo|Close all/)).toBeNull();
    });

    it("collapses to the newest with a +2 button", () => {
      mount();
      three();
      expect(qt("uno")).toBeNull();
      expect(qt("dos")).toBeNull();
      expect(gt("tres")).not.toBeNull();
      const more = screen.getByRole("button", { name: MORE });
      expect(more.getAttribute("aria-expanded")).toBe("false");
      expect(more.textContent).toBe("+2");
    });

    it("expands on hover, focus or click and collapses again", () => {
      mount();
      three();
      fireEvent.mouseEnter(host());
      expect(gt("uno")).not.toBeNull();
      fireEvent.mouseLeave(host());
      expect(qt("uno")).toBeNull();

      fireEvent.focus(screen.getByRole("button", { name: MORE }));
      expect(gt("uno")).not.toBeNull();
      fireEvent.blur(screen.getAllByRole("button", { name: /^(Cerrar|Close)$/ })[0]);
      expect(qt("uno")).toBeNull();

      fireEvent.click(screen.getByRole("button", { name: MORE }));
      expect(gt("uno")).not.toBeNull();
      expect(gt("dos")).not.toBeNull();
      fireEvent.mouseLeave(host());
      fireEvent.blur(host());
      expect(qt("uno")).toBeNull();
    });

    it("Cerrar todo dismisses everything and calls each onDismiss", () => {
      mount();
      const a = vi.fn(), b = vi.fn();
      act(() => {
        toast({ text: "uno", kind: "error", onDismiss: a });
        toast({ key: "w", text: "dos", kind: "wait", onDismiss: b });
      });
      fireEvent.mouseEnter(host());
      fireEvent.click(gt(/Cerrar todo|Close all/));
      expect(a).toHaveBeenCalledTimes(1);
      expect(b).toHaveBeenCalledTimes(1);
      expect(document.querySelector(".toast-host")).toBeNull();
    });

    it("announces through one status region; the stack has no live role", () => {
      mount();
      act(() => void toast({ text: "hola", kind: "error" }));
      act(() => void toast({ text: "hola", kind: "error" }));
      const regions = screen.getAllByRole("status");
      expect(regions.length).toBe(1);
      expect(regions[0].textContent).toBe("hola ×2");
      act(() => void toast({ key: "k", text: "clave", kind: "wait" }));
      expect(regions[0].textContent).toBe("clave");
    });
  });

  describe("pause while expanded", () => {
    const host = () => document.querySelector(".toast-host") as HTMLElement;
    const tick = (ms: number) => act(() => void vi.advanceTimersByTime(ms));

    it("hovering freezes the timer and leaving resumes the remaining time", () => {
      mount();
      act(() => void toast({ text: "pronto" }));
      tick(4000);
      fireEvent.mouseEnter(host());
      tick(10000);
      expect(qt("pronto")).not.toBeNull();
      fireEvent.mouseLeave(host());
      tick(1900);
      expect(qt("pronto")).not.toBeNull();
      tick(200);
      expect(qt("pronto")).toBeNull();
    });

    it("hidden older info toasts keep expiring while not hovered", () => {
      mount();
      act(() => {
        toast({ text: "viejo", ttl: 1000 });
        toast({ text: "nuevo", kind: "error" });
      });
      expect(qt("viejo")).toBeNull();
      tick(1100);
      fireEvent.mouseEnter(host());
      expect(qt("viejo")).toBeNull();
      expect(gt("nuevo")).not.toBeNull();
    });

    it("a toast pushed while expanded gets its full ttl after leaving", () => {
      mount();
      act(() => void toast({ text: "a", kind: "error" }));
      fireEvent.mouseEnter(host());
      act(() => void toast({ text: "b", ttl: 2000 }));
      tick(10000);
      expect(gt("b")).not.toBeNull();
      fireEvent.mouseLeave(host());
      tick(1900);
      expect(qt("b")).not.toBeNull();
      tick(200);
      expect(qt("b")).toBeNull();
    });

    it("unmounting the host resumes the timers", () => {
      mount();
      act(() => void toast({ text: "u", ttl: 1000 }));
      fireEvent.mouseEnter(host());
      cleanup();
      tick(1100);
      mount();
      expect(qt("u")).toBeNull();
    });
  });
});
