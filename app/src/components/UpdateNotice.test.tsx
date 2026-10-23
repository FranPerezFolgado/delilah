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
    fireEvent.click(screen.getByText("Ver novedades"));
    expect(screen.getByText("Nuevo").tagName).toBe("STRONG");
    fireEvent.click(screen.getByText("Actualizar"));
    expect(dispatch).toHaveBeenCalledWith({ type: "update", running: 0 });
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
    fireEvent.click(screen.getByText("Cerrar"));
    expect(dispatch).toHaveBeenCalledWith({ type: "close" });
  });
});
