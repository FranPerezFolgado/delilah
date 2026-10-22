// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/react";
import { setLanguage } from "../i18n";
import { AskCard } from "./AskCard";

beforeEach(() => setLanguage("es"));

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const withPreview = {
  id: "a1",
  questions: [
    {
      question: "Which?",
      options: [
        { label: "One", preview: "**first-preview**" },
        { label: "Two", preview: "**second-preview**" },
      ],
    },
  ],
};

const proposal = {
  id: "a2",
  questions: [{ question: "¿Creo la tarea?", options: [{ label: "Crear" }, { label: "Cambiar algo" }, { label: "Descartar" }] }],
  proposal: { title: "Reintentos", brief: "El **brief** completo" },
};

describe("AskCard", () => {
  it("shows the first option's preview before any pick, then the picked one", () => {
    const { container, getByText } = render(<AskCard ask={{ ...withPreview, questions: [{ ...withPreview.questions[0], multiSelect: true }] }} />);
    expect(container.querySelector(".ask-preview strong")?.textContent).toBe("first-preview");
    fireEvent.click(getByText("Two"));
    expect(container.querySelector(".ask-preview strong")?.textContent).toBe("second-preview");
  });

  it("renders a proposal through ProposalCard", () => {
    const { getByRole, getByText } = render(<AskCard ask={proposal} />);
    expect(getByRole("heading").textContent).toBe("Reintentos");
    for (const b of ["Crear", "Cambiar algo", "Descartar"]) expect(getByText(b)).toBeTruthy();
  });
});
