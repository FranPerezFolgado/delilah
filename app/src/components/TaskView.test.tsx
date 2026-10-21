// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { cleanup } from "@testing-library/react";
import { setLanguage, t } from "../i18n";
import { stepLabel } from "../chain";

beforeEach(() => setLanguage("es"));

afterEach(() => {
  cleanup();
  localStorage.clear();
});

describe("step-started history label", () => {
  it("names the step, not a phase", () => {
    const label = (step: "plan" | "implement" | "review") => t("Paso a {step} (sesión nueva)", { step: stepLabel(step) });
    expect(label("plan")).toBe("Paso a Spec y plan (sesión nueva)");
    expect(label("implement")).toBe("Paso a Implementación (sesión nueva)");
    expect(label("review")).toBe("Paso a Revisión (sesión nueva)");
  });

  it("translates the step labels", () => {
    setLanguage("en");
    expect(stepLabel("plan")).toBe("Spec and plan");
    expect(t("Paso a {step} (sesión nueva)", { step: stepLabel("review") })).toBe("Moved to Review (new session)");
  });
});
