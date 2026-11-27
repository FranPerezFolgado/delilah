// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, renderHook, screen, act } from "@testing-library/react";
import { CloseButton, Notice, useDismissal, useDismissals } from "./Notice";

import { setLanguage } from "../../i18n";
beforeEach(() => setLanguage("es"));
afterEach(cleanup);

describe("CloseButton", () => {
  it("is a Cerrar icon button with the ntc-x class that calls onClick", () => {
    const onClick = vi.fn();
    render(<CloseButton onClick={onClick} />);
    const b = screen.getByRole("button", { name: "Cerrar" });
    expect(b.classList.contains("ntc-x")).toBe(true);
    expect(b.querySelector("svg")).not.toBeNull();
    fireEvent.click(b);
    expect(onClick).toHaveBeenCalledTimes(1);
  });

  it("merges a custom className with ntc-x", () => {
    render(<CloseButton onClick={() => {}} className="extra" />);
    const b = screen.getByRole("button", { name: "Cerrar" });
    expect(b.classList.contains("ntc-x")).toBe(true);
    expect(b.classList.contains("extra")).toBe(true);
  });
});

describe("Notice", () => {
  it("renders children then exactly one close button, keeping className and role", () => {
    const onClose = vi.fn();
    render(
      <Notice className="gn-bar" role="alert" onClose={onClose}>
        <span>hola</span>
        <button>Accion</button>
      </Notice>,
    );
    const root = screen.getByRole("alert");
    expect(root.classList.contains("gn-bar")).toBe(true);
    expect(root.classList.contains("ntc")).toBe(true);
    expect(screen.getAllByRole("button", { name: "Cerrar" })).toHaveLength(1);
    expect(root.lastElementChild).toBe(screen.getByRole("button", { name: "Cerrar" }));
    fireEvent.click(screen.getByRole("button", { name: "Cerrar" }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("moves focus to the next focusable element after the notice", () => {
    render(
      <div>
        <Notice onClose={() => {}}>x</Notice>
        <button>siguiente</button>
      </div>,
    );
    const x = screen.getByRole("button", { name: "Cerrar" });
    x.focus();
    fireEvent.click(x);
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "siguiente" }));
  });

  it("skips focus candidates inside an inert subtree", () => {
    render(
      <div>
        <Notice onClose={() => {}}>x</Notice>
        <div inert>
          <button>oculto</button>
        </div>
        <button>visible</button>
      </div>,
    );
    const x = screen.getByRole("button", { name: "Cerrar" });
    x.focus();
    fireEvent.click(x);
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "visible" }));
  });

  it("with minimal children renders just the one close button", () => {
    render(<Notice onClose={() => {}}>solo texto</Notice>);
    expect(screen.getAllByRole("button")).toHaveLength(1);
    expect(screen.getByRole("button", { name: "Cerrar" })).toBeTruthy();
  });
});

describe("useDismissals", () => {
  it("hides only dismissed ids while the signature is unchanged", () => {
    const { result, rerender } = renderHook(({ c }) => useDismissals(c), { initialProps: { c: { a: "1", b: "1" } } });
    act(() => result.current.dismiss("a"));
    expect(result.current.hidden("a")).toBe(true);
    expect(result.current.hidden("b")).toBe(false);
    rerender({ c: { a: "1", b: "1" } });
    expect(result.current.hidden("a")).toBe(true);
  });

  it("forgets on signature change, so A -> B -> A shows again", () => {
    const { result, rerender } = renderHook(({ c }) => useDismissals(c), { initialProps: { c: { a: "A" } } });
    act(() => result.current.dismiss("a"));
    rerender({ c: { a: "B" } });
    expect(result.current.hidden("a")).toBe(false);
    rerender({ c: { a: "A" } });
    expect(result.current.hidden("a")).toBe(false);
  });
});

describe("useDismissal", () => {
  it("null forgets the dismissal", () => {
    const { result, rerender } = renderHook(({ s }) => useDismissal(s), { initialProps: { s: "x" as string | null } });
    act(() => result.current[1]());
    expect(result.current[0]).toBe(true);
    rerender({ s: null });
    expect(result.current[0]).toBe(false);
    rerender({ s: "x" });
    expect(result.current[0]).toBe(false);
  });
});
