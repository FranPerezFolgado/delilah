// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { api } from "../api";
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

  it("renders a proposal with title, open brief and three buttons", () => {
    const { getByRole, getByText } = render(<AskCard ask={proposal} />);
    expect(getByRole("heading").textContent).toBe("Reintentos");
    expect(getByText("brief").tagName).toBe("STRONG");
    for (const b of ["Crear", "Cambiar algo", "Descartar"]) expect(getByText(b)).toBeTruthy();
  });

  it("Crear answers and shows the created task as a link", async () => {
    const answer = vi.spyOn(api, "answerAsk").mockResolvedValue({ task: "LIBRE-7" });
    const onOpenTask = vi.fn();
    const { getByText, findByText } = render(<AskCard ask={proposal} onOpenTask={onOpenTask} />);
    fireEvent.click(getByText("Crear"));
    const link = await findByText("LIBRE-7");
    expect(answer).toHaveBeenCalledWith("a2", { "¿Creo la tarea?": "Crear" });
    expect(link.tagName).toBe("A");
    expect(link.parentElement?.textContent).toContain("Creada LIBRE-7");
    fireEvent.click(link);
    expect(onOpenTask).toHaveBeenCalledWith("LIBRE-7");
  });

  it("shows an error response", async () => {
    vi.spyOn(api, "answerAsk").mockResolvedValue({ task: null, error: "no se pudo crear" });
    const { getByText, findByText } = render(<AskCard ask={proposal} />);
    fireEvent.click(getByText("Crear"));
    expect(await findByText("no se pudo crear")).toBeTruthy();
    await waitFor(() => expect((getByText("Crear") as HTMLButtonElement).disabled).toBe(false));
  });
});
