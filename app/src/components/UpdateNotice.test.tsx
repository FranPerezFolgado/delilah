// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { setLanguage } from "../i18n";
import { UpdateNotice } from "./UpdateNotice";
import type { UpdateState } from "../updates";

function setup(state: UpdateState) {
  const dispatch = vi.fn();
  const view = render(<UpdateNotice state={state} dispatch={dispatch} />);
  return { dispatch, view };
}

beforeEach(() => setLanguage("es"));
afterEach(cleanup);

describe("UpdateNotice", () => {
  it("renders nothing when idle", () => {
    const { view } = setup({ kind: "idle" });
    expect(view.container.firstChild).toBeNull();
  });

  it("offers an available version with notes and dispatches its buttons", () => {
    const { dispatch } = setup({ kind: "available", version: "1.2.0", notes: "**Nuevo** algo" });
    expect(screen.getByText("Delilah 1.2.0 disponible")).toBeTruthy();
    fireEvent.click(screen.getByText("Abrir novedades"));
    expect(screen.getByText("Nuevo").tagName).toBe("STRONG");
    fireEvent.click(screen.getByText("Actualizar"));
    expect(dispatch).toHaveBeenCalledWith({ type: "requestUpdate" });
    fireEvent.click(screen.getByText("Más tarde"));
    expect(dispatch).toHaveBeenCalledWith({ type: "later" });
  });

  it("shows download progress, or an indeterminate one", () => {
    const { view } = setup({ kind: "downloading", version: "1.2.0", received: 50, total: 200 });
    expect(screen.getByText("Descargando… 25%")).toBeTruthy();
    view.unmount();
    setup({ kind: "downloading", version: "1.2.0", received: 50, total: null });
    expect(screen.getByText("Descargando…")).toBeTruthy();
  });

  it("shows installing", () => {
    setup({ kind: "installing", version: "1.2.0" });
    expect(screen.getByText("Instalando y reiniciando…")).toBeTruthy();
  });

  it("shows the error with Reintentar and Cerrar", () => {
    const { dispatch } = setup({ kind: "error", reason: "sin red", manual: true, version: "1.2.0" });
    expect(screen.getByText("No se pudo actualizar: sin red")).toBeTruthy();
    fireEvent.click(screen.getByText("Reintentar"));
    expect(dispatch).toHaveBeenCalledWith({ type: "retry" });
    fireEvent.click(screen.getByRole("button", { name: "Cerrar" }));
    expect(dispatch).toHaveBeenCalledWith({ type: "close" });
  });

  it("confirms before stopping sessions", () => {
    const { dispatch } = setup({ kind: "confirmSessions", version: "1.2.0", notes: "", running: 2 });
    expect(screen.getByText("Hay 2 sesiones en marcha; se detendrán al actualizar")).toBeTruthy();
    fireEvent.click(screen.getByText("Actualizar ahora"));
    expect(dispatch).toHaveBeenCalledWith({ type: "now" });
    fireEvent.click(screen.getByText("Cuando terminen"));
    expect(dispatch).toHaveBeenCalledWith({ type: "whenIdle" });
    fireEvent.click(screen.getByText("Cancelar"));
    expect(dispatch).toHaveBeenCalledWith({ type: "cancel" });
  });

  it("uses the singular for one session", () => {
    setup({ kind: "confirmSessions", version: "1.2.0", notes: "", running: 1 });
    expect(screen.getByText("Hay 1 sesión en marcha; se detendrá al actualizar")).toBeTruthy();
  });

  it("shows the wait for sessions to finish", () => {
    const { dispatch } = setup({ kind: "waiting", version: "1.2.0", notes: "", running: 3 });
    expect(screen.getByText("Se actualizará cuando terminen 3 sesiones")).toBeTruthy();
    fireEvent.click(screen.getByText("Actualizar ahora"));
    expect(dispatch).toHaveBeenCalledWith({ type: "now" });
    fireEvent.click(screen.getByText("Cancelar"));
    expect(dispatch).toHaveBeenCalledWith({ type: "cancel" });
  });

  it("shows the reason of a failed check, and a verification failure apart", () => {
    const { view } = setup({ kind: "error", reason: "sin red", manual: true });
    expect(screen.getByText("No se pudo buscar actualizaciones: sin red")).toBeTruthy();
    view.unmount();
    setup({ kind: "error", reason: "x", manual: true, version: "1.2.0" });
    expect(screen.queryByText("No se pudo buscar actualizaciones: x")).toBeNull();
  });

  it("says up to date with the installed version", () => {
    const dispatch = vi.fn();
    render(<UpdateNotice state={{ kind: "upToDate", manual: true }} installed="1.2.0" dispatch={dispatch} />);
    expect(screen.getByText("Delilah está al día (1.2.0)")).toBeTruthy();
  });

  it("offers Reintentar on a failed manual check too", () => {
    const { dispatch } = setup({ kind: "error", reason: "sin red", manual: true });
    fireEvent.click(screen.getByText("Reintentar"));
    expect(dispatch).toHaveBeenCalledWith({ type: "retry" });
  });

  const X = () => screen.getByRole("button", { name: "Cerrar" });

  it("upToDate: the X dispatches close and no text Cerrar button remains", () => {
    const { dispatch } = setup({ kind: "upToDate", manual: true });
    expect(screen.getAllByRole("button", { name: "Cerrar" })).toHaveLength(1);
    expect(X().className).toContain("ntc-x");
    fireEvent.click(X());
    expect(dispatch).toHaveBeenCalledWith({ type: "close" });
  });

  it("error: only the icon X is named Cerrar", () => {
    setup({ kind: "error", reason: "x", manual: true, version: "1.2.0" });
    expect(screen.getAllByRole("button", { name: "Cerrar" })).toHaveLength(1);
  });

  it("available: the X dispatches later and Más tarde stays", () => {
    const { dispatch } = setup({ kind: "available", version: "1.2.0", notes: "" });
    expect(screen.getByText("Más tarde")).toBeTruthy();
    fireEvent.click(X());
    expect(dispatch).toHaveBeenCalledWith({ type: "later" });
  });

  it("confirmSessions: the X dispatches cancel and Cancelar stays", () => {
    const { dispatch } = setup({ kind: "confirmSessions", version: "1.2.0", notes: "", running: 2 });
    expect(screen.getByText("Cancelar")).toBeTruthy();
    fireEvent.click(X());
    expect(dispatch).toHaveBeenCalledWith({ type: "cancel" });
  });

  it.each<[string, UpdateState, string]>([
    ["checking", { kind: "checking", manual: true }, "Buscando actualizaciones…"],
    ["downloading", { kind: "downloading", version: "1.2.0", received: 1, total: 10 }, "Descargando… 10%"],
    ["installing", { kind: "installing", version: "1.2.0" }, "Instalando y reiniciando…"],
    ["waiting", { kind: "waiting", version: "1.2.0", notes: "", running: 1 }, "Se actualizará cuando termine 1 sesión"],
  ])("%s: the X hides the bar without dispatching", (_k, state, text) => {
    const { dispatch } = setup(state);
    expect(screen.getByText(text)).toBeTruthy();
    fireEvent.click(X());
    expect(dispatch).not.toHaveBeenCalled();
    expect(screen.queryByText(text)).toBeNull();
  });

  it("stays hidden on a progress update and returns on another kind", () => {
    const dispatch = vi.fn();
    const { rerender } = render(<UpdateNotice state={{ kind: "downloading", version: "1.2.0", received: 1, total: 10 }} dispatch={dispatch} />);
    fireEvent.click(X());
    rerender(<UpdateNotice state={{ kind: "downloading", version: "1.2.0", received: 5, total: 10 }} dispatch={dispatch} />);
    expect(screen.queryByRole("status")).toBeNull();
    rerender(<UpdateNotice state={{ kind: "installing", version: "1.2.0" }} dispatch={dispatch} />);
    expect(screen.getByText("Instalando y reiniciando…")).toBeTruthy();
  });
});
