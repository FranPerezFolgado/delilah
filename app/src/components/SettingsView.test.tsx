// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { setLanguage } from "../i18n";
import { DEFAULTS } from "../shortcuts";
import type { UpdateMeta } from "../updates";

vi.mock("../api", () => ({
  ASK_POSITIONS: ["top", "right", "bottom"],
  api: { preference: vi.fn().mockResolvedValue(null), setPreference: vi.fn() },
}));

import { SettingsView } from "./SettingsView";

const meta = (over: Partial<UpdateMeta> = {}): UpdateMeta => ({
  installed: "1.2.0", lastCheck: null, lastResult: null, dismissed: null, ...over,
});

function setup(updates?: Parameters<typeof SettingsView>[0]["updates"]) {
  return render(
    <SettingsView bindings={{ ...DEFAULTS }} onChange={() => {}} language="es" onLanguage={() => {}}
      askPosition="bottom" onAskPosition={() => {}} updates={updates} />,
  );
}

beforeEach(() => setLanguage("es"));
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
