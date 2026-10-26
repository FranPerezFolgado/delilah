// @vitest-environment jsdom
// The language resolution table of specs/008-public-repo-onboarding/contracts/language.md.
// It is mirrored in the `english_for` tests in app/src-tauri/src/daemon.rs.
import { afterEach, describe, expect, it, vi } from "vitest";
import { detect, lang, setLanguage } from "./i18n";

const MISSING = Symbol("missing");
const prefs: unknown[] = [MISSING, null, "auto", "fr", 42, "es", "en"];
const systems: (string | undefined)[] = ["es-ES", "es-419", "ES", "en-US", "fr-FR", "ja-JP", "", undefined];

function expected(pref: unknown, system: string | undefined) {
  if (pref === "es" || pref === "en") return pref;
  return system && system.toLowerCase().startsWith("es") ? "es" : "en";
}

afterEach(() => vi.unstubAllGlobals());

describe.each(systems)("system %j", (system) => {
  it.each(prefs.map((p) => [p === MISSING ? "missing" : JSON.stringify(p), p]))(
    "pref %s",
    (_name, pref) => {
      if (system === undefined) vi.stubGlobal("navigator", undefined);
      else vi.stubGlobal("navigator", { language: system });
      const value = pref === MISSING ? undefined : pref;
      expect(detect(value)).toBe(expected(pref, system));
    },
  );
});

describe("setLanguage", () => {
  it("falls back to English for an unknown value on an English system", () => {
    vi.stubGlobal("navigator", { language: "en-US" });
    setLanguage("fr");
    expect(lang()).toBe("en");
    expect(document.documentElement.lang).toBe("en");
  });

  it("keeps document lang in sync", () => {
    setLanguage("es");
    expect(document.documentElement.lang).toBe("es");
    setLanguage("en");
    expect(document.documentElement.lang).toBe("en");
  });
});

describe("module load", () => {
  it("sets document lang from the start", async () => {
    vi.resetModules();
    vi.stubGlobal("navigator", { language: "fr-FR" });
    document.documentElement.lang = "xx";
    await import("./i18n");
    expect(document.documentElement.lang).toBe("en");
  });
});
