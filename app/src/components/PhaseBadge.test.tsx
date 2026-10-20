// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { cleanup, render } from "@testing-library/react";
import { setLanguage } from "../i18n";
import { PhaseBadge } from "./PhaseBadge";

beforeEach(() => setLanguage("es"));

afterEach(() => {
  cleanup();
  localStorage.clear();
});

describe("PhaseBadge", () => {
  it("shows the step and the phase name in a phase pill", () => {
    const { container } = render(<PhaseBadge phase={{ column: "plan", step: 2 }} />);
    const pill = container.querySelector(".pill.phase-pill");
    expect(pill).not.toBeNull();
    expect(pill!.querySelector(".dot-step")!.textContent).toBe("2");
    expect(pill!.textContent).toBe("2Plan");
  });

  it("labels it for assistive tech and as a tooltip", () => {
    const { container } = render(<PhaseBadge phase={{ column: "plan", step: 2 }} />);
    const pill = container.querySelector(".phase-pill")!;
    expect(pill.getAttribute("aria-label")).toBe("Fase 2: Plan");
    expect(pill.getAttribute("title")).toBe("Fase: Plan");
  });

  it("is not clickable", () => {
    const { container } = render(<PhaseBadge phase={{ column: "implement", step: 3 }} />);
    expect(container.querySelector("button, a, [role=button]")).toBeNull();
  });
});
