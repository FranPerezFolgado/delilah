// @vitest-environment jsdom
import { act } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { ToastHost, useToast, clearToasts } from "./Toast";

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
    expect(screen.getByRole("status").textContent).toContain("Hecho");
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
    expect(screen.getAllByRole("status").length).toBe(3);
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
});
