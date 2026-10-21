// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/react";
import { api } from "../api";
import { setLanguage } from "../i18n";
import { ProposalCard } from "./ProposalCard";

beforeEach(() => setLanguage("es"));

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const proposal = {
  id: "a2",
  questions: [{ question: "¿Creo la tarea?" }],
  proposal: { title: "Reintentos", brief: "El **brief** completo" },
};

describe("ProposalCard", () => {
  it("renders title, open brief and three buttons", () => {
    const { getByRole, getByText } = render(<ProposalCard ask={proposal} />);
    expect(getByRole("heading").textContent).toBe("Reintentos");
    expect(getByText("brief").tagName).toBe("STRONG");
    for (const b of ["Crear", "Cambiar algo", "Descartar"]) expect(getByText(b)).toBeTruthy();
  });

  it("Crear answers and shows the created task as a link", async () => {
    const answer = vi.spyOn(api, "answerAsk").mockResolvedValue({ task: "LIBRE-7" });
    const onOpenTask = vi.fn();
    const onDone = vi.fn();
    const { getByText, findByText } = render(<ProposalCard ask={proposal} onOpenTask={onOpenTask} onDone={onDone} />);
    fireEvent.click(getByText("Crear"));
    const link = await findByText("LIBRE-7");
    expect(answer).toHaveBeenCalledWith("a2", { "¿Creo la tarea?": "Crear" });
    expect(link.tagName).toBe("A");
    expect(link.parentElement?.textContent).toContain("Creada LIBRE-7");
    expect(onDone).toHaveBeenCalled();
    fireEvent.click(link);
    expect(onOpenTask).toHaveBeenCalledWith("LIBRE-7");
  });

  it("shows an error response, stops offering the buttons and still calls onDone", async () => {
    vi.spyOn(api, "answerAsk").mockResolvedValue({ task: null, error: "no se pudo crear" });
    const onDone = vi.fn();
    const { getByText, findByText, queryByText } = render(<ProposalCard ask={proposal} onDone={onDone} />);
    fireEvent.click(getByText("Crear"));
    expect(await findByText("no se pudo crear")).toBeTruthy();
    expect(queryByText("Crear")).toBeNull();
    expect(queryByText("Descartar")).toBeNull();
    expect(onDone).toHaveBeenCalled();
  });
});
