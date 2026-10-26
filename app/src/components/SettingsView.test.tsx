// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { setLanguage } from "../i18n";
import { DEFAULTS } from "../shortcuts";
import type { UpdateMeta } from "../updates";

const platform = vi.hoisted(() => ({ isDesktop: false, revealInFinder: vi.fn().mockResolvedValue(undefined) }));
vi.mock("../platform", () => platform);
vi.mock("../api", () => ({
  ASK_POSITIONS: ["top", "right", "bottom"],
  api: {
    preference: vi.fn().mockResolvedValue(null),
    setPreference: vi.fn().mockResolvedValue(undefined),
    health: vi.fn().mockResolvedValue({ version: "0.9.1", home: "/Users/x/.delilah" }),
  },
}));
import { api } from "../api";

import { SettingsView } from "./SettingsView";

const meta = (over: Partial<UpdateMeta> = {}): UpdateMeta => ({
  installed: "1.2.0", lastCheck: null, lastResult: null, dismissed: null, ...over,
});

function setup(updates?: Parameters<typeof SettingsView>[0]["updates"], bindings = { ...DEFAULTS }) {
  return render(
    <SettingsView bindings={bindings} onChange={() => {}} language="es" onLanguage={() => {}}
      askPosition="bottom" onAskPosition={() => {}} updates={updates} />,
  );
}

beforeEach(() => {
  setLanguage("es");
  platform.isDesktop = false;
  platform.revealInFinder.mockClear();
});
afterEach(cleanup);

describe("SettingsView updates section", () => {
  it("shows the version, that there was no check yet, and checks on demand", () => {
    const check = vi.fn();
    setup({ meta: meta(), canUpdate: true, check });
    expect(screen.getByText("Versión 1.2.0")).toBeTruthy();
    expect(screen.getByText("Última comprobación: Todavía no")).toBeTruthy();
    fireEvent.click(screen.getByText("Buscar ahora"));
    expect(check).toHaveBeenCalledWith(true);
  });

  it("shows the time and result of the last check", () => {
    const lastCheck = new Date("2026-10-24T10:05:00");
    setup({ meta: meta({ lastCheck, lastResult: "upToDate" }), canUpdate: true, check: vi.fn() });
    const row = screen.getByText(/^Última comprobación: /).textContent!;
    expect(row).toContain("Delilah está al día");
    expect(row).not.toContain("Todavía no");
  });

  it("says when the last check failed", () => {
    setup({ meta: meta({ lastCheck: new Date(), lastResult: "error" }), canUpdate: true, check: vi.fn() });
    expect(screen.getByText(/^Última comprobación: .*falló/)).toBeTruthy();
  });

  it("without canUpdate it explains and has no button", () => {
    setup({ meta: meta(), canUpdate: false, check: vi.fn() });
    expect(screen.getByText("Las actualizaciones solo están en la app instalada")).toBeTruthy();
    expect(screen.queryByText("Buscar ahora")).toBeNull();
  });

  it("is absent without the prop", () => {
    setup();
    expect(screen.queryByText("Buscar ahora")).toBeNull();
  });
});

describe("SettingsView structure", () => {
  it("has the section index with the six sections", () => {
    setup();
    const nav = screen.getByRole("navigation");
    expect([...nav.querySelectorAll("button")].map((b) => b.textContent)).toEqual([
      "General", "Preguntas", "Modo ahorro", "Actualizaciones", "Atajos de teclado", "Diagnóstico",
    ]);
  });

  it("shows «Por defecto» only on changed shortcuts and «Restaurar todos» is disabled with none", () => {
    setup();
    expect(screen.queryByText("Por defecto")).toBeNull();
    expect((screen.getByText("Restaurar todos") as HTMLButtonElement).disabled).toBe(true);
    cleanup();
    const first = Object.keys(DEFAULTS)[0] as keyof typeof DEFAULTS;
    setup(undefined, { ...DEFAULTS, [first]: "Mod+Alt+Shift+9" } as typeof DEFAULTS);
    expect(screen.getAllByText("Por defecto")).toHaveLength(1);
    expect((screen.getByText("Restaurar todos") as HTMLButtonElement).disabled).toBe(false);
  });

  it("shows the capture state while changing a shortcut and Esc cancels", () => {
    setup();
    fireEvent.click(screen.getAllByText("Cambiar")[0]);
    expect(screen.getByText("Pulsa la nueva combinación… · Esc para cancelar")).toBeTruthy();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByText("Pulsa la nueva combinación… · Esc para cancelar")).toBeNull();
  });

  it("uses the Modo ahorro wording and «Control remoto»", () => {
    setup();
    expect(screen.getByText("Una sesión de Claude nueva por fase, para gastar menos contexto")).toBeTruthy();
    expect(screen.getByText("Control remoto")).toBeTruthy();
  });

  it("saves the auto-update preference (on by default)", async () => {
    setup();
    const toggle = await screen.findByRole("button", { name: "Buscar actualizaciones automáticamente" });
    await waitFor(() => expect((toggle as HTMLButtonElement).disabled).toBe(false));
    expect(toggle.getAttribute("aria-pressed")).toBe("true");
    fireEvent.click(toggle);
    await waitFor(() => expect(api.setPreference).toHaveBeenCalledWith("auto-update", false));
    await waitFor(() => expect(toggle.getAttribute("aria-pressed")).toBe("false"));
  });
});

describe("SettingsView agent permissions", () => {
  const prefIs = (v: unknown) =>
    vi.mocked(api.preference).mockImplementation(((k: string) => Promise.resolve(k === "agent-permissions" ? v : null)) as never);
  const radio = (name: string) => screen.findByRole("radio", { name });

  afterEach(() => {
    vi.mocked(api.preference).mockResolvedValue(null);
    vi.mocked(api.setPreference).mockResolvedValue(undefined);
  });

  it("shows Auto selected when the preference is null or auto", async () => {
    prefIs(null);
    setup();
    expect(screen.getByText("Permisos de los agentes")).toBeTruthy();
    expect((await radio("Auto")).getAttribute("aria-checked")).toBe("true");
    cleanup();
    prefIs("auto");
    setup();
    expect((await radio("Auto")).getAttribute("aria-checked")).toBe("true");
  });

  it("shows «Por defecto de Claude» when the preference is default", async () => {
    prefIs("default");
    setup();
    const def = await radio("Por defecto de Claude");
    await waitFor(() => expect(def.getAttribute("aria-checked")).toBe("true"));
    expect((await radio("Auto")).getAttribute("aria-checked")).toBe("false");
  });

  it("saves the chosen option", async () => {
    prefIs(null);
    setup();
    fireEvent.click(await radio("Por defecto de Claude"));
    await waitFor(() => expect(api.setPreference).toHaveBeenCalledWith("agent-permissions", "default"));
    await waitFor(() => expect(screen.getByRole("radio", { name: "Por defecto de Claude" }).getAttribute("aria-checked")).toBe("true"));
  });

  it("on a failed save shows the error and keeps the previous choice", async () => {
    prefIs("auto");
    vi.mocked(api.setPreference).mockRejectedValue(new Error("boom"));
    setup();
    fireEvent.click(await radio("Por defecto de Claude"));
    expect(await screen.findByText("boom")).toBeTruthy();
    expect(screen.getByRole("radio", { name: "Auto" }).getAttribute("aria-checked")).toBe("true");
    expect(screen.getByRole("radio", { name: "Por defecto de Claude" }).getAttribute("aria-checked")).toBe("false");
  });

  it("on a failed load shows the error and keeps the control disabled", async () => {
    vi.mocked(api.preference).mockImplementation(((k: string) =>
      k === "agent-permissions" ? Promise.reject(new Error("down")) : Promise.resolve(null)) as never);
    setup();
    expect(await screen.findByText("down")).toBeTruthy();
    fireEvent.click(await radio("Por defecto de Claude"));
    expect(api.setPreference).not.toHaveBeenCalledWith("agent-permissions", "default");
  });
});

describe("SettingsView diagnostics", () => {
  it("«Abrir» asks the app to open the logs", () => {
    const seen = vi.fn();
    window.addEventListener("delilah:show-logs", seen);
    setup();
    fireEvent.click(screen.getByText("Abrir"));
    window.removeEventListener("delilah:show-logs", seen);
    expect(seen).toHaveBeenCalled();
  });

  it("shows the daemon version and data folder; no Finder button outside the desktop", async () => {
    setup();
    expect(await screen.findByText("0.9.1")).toBeTruthy();
    expect(screen.getByText("/Users/x/.delilah")).toBeTruthy();
    expect(screen.queryByText("Mostrar en Finder")).toBeNull();
  });

  it("on the desktop «Mostrar en Finder» reveals the data folder", async () => {
    platform.isDesktop = true;
    setup();
    fireEvent.click(await screen.findByText("Mostrar en Finder"));
    expect(platform.revealInFinder).toHaveBeenCalledWith("/Users/x/.delilah");
  });
});
