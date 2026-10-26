// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { api, ApiError } from "../api";
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

const single = { id: "a3", questions: [{ question: "¿Cuál?", options: [{ label: "Uno" }, { label: "Dos" }, { label: "Tres" }] }] };
const permission = { id: "p1", tool: "Bash", summary: "npm test", suggestion: null, rule: "Bash(npm test)" };
const btn = (name: string) => screen.getByText(name).closest("button") as HTMLButtonElement;

describe("AskCard: Pregunta", () => {
  it("shows the first option's preview before any pick, then the picked one", () => {
    const { container, getByText } = render(<AskCard ask={{ ...withPreview, questions: [{ ...withPreview.questions[0], multiSelect: true }] }} />);
    expect(container.querySelector(".ac-preview strong")?.textContent).toBe("first-preview");
    fireEvent.click(getByText("Two"));
    expect(container.querySelector(".ac-preview strong")?.textContent).toBe("second-preview");
  });

  it("renders a proposal through ProposalCard", () => {
    const { getByRole, getByText } = render(<AskCard ask={proposal} />);
    expect(getByRole("heading").textContent).toBe("Reintentos");
    for (const b of ["Crear", "Cambiar algo", "Descartar"]) expect(getByText(b)).toBeTruthy();
  });

  it("numbers the options and answers on the first click when there is one question", async () => {
    const spy = vi.spyOn(api, "answerAsk").mockResolvedValue(null as never);
    const done = vi.fn();
    const { container } = render(<AskCard ask={single} onDone={done} />);
    expect([...container.querySelectorAll(".ac-num")].map((n) => n.textContent)).toEqual(["1", "2", "3"]);
    fireEvent.click(screen.getByText("Dos"));
    await waitFor(() => expect(done).toHaveBeenCalled());
    expect(spy).toHaveBeenCalledWith("a3", { "¿Cuál?": "Dos" });
  });

  it("sends a free reply with Enviar and with ⌘↵", async () => {
    const spy = vi.spyOn(api, "answerAsk").mockResolvedValue(null as never);
    render(<AskCard ask={single} />);
    expect(btn("Enviar").disabled).toBe(true);
    const input = screen.getByLabelText("Tu respuesta a: ¿Cuál?");
    fireEvent.change(input, { target: { value: "otra cosa" } });
    fireEvent.keyDown(input, { key: "Enter", metaKey: true });
    await waitFor(() => expect(spy).toHaveBeenCalledWith("a3", { "¿Cuál?": "otra cosa" }));
    expect(btn("Enviar").textContent).toContain("Enviar");
  });

  it("shows the error inline and the way out to the terminal", async () => {
    vi.spyOn(api, "answerAsk").mockRejectedValue(new Error("daemon down"));
    render(<AskCard ask={single} />);
    fireEvent.click(screen.getByText("Uno"));
    expect(await screen.findByText("daemon down")).toBeTruthy();
    expect(screen.getByText("Responder en la terminal")).toBeTruthy();
  });

  it("keys 1-3 choose an option, but not while typing, and the hint says so", async () => {
    const spy = vi.spyOn(api, "answerAsk").mockResolvedValue(null as never);
    render(<AskCard ask={single} keys />);
    expect(screen.getByText(/fuera de los campos de texto/)).toBeTruthy();
    fireEvent.keyDown(screen.getByLabelText("Tu respuesta a: ¿Cuál?"), { key: "2" });
    expect(spy).not.toHaveBeenCalled();
    fireEvent.keyDown(document.body, { key: "3" });
    await waitFor(() => expect(spy).toHaveBeenCalledWith("a3", { "¿Cuál?": "Tres" }));
  });

  it("does not listen to keys unless asked", () => {
    const spy = vi.spyOn(api, "answerAsk").mockResolvedValue(null as never);
    render(<AskCard ask={single} />);
    fireEvent.keyDown(document.body, { key: "1" });
    expect(spy).not.toHaveBeenCalled();
    expect(screen.queryByText(/fuera de los campos de texto/)).toBeNull();
  });
});

describe("AskCard: Permiso", () => {
  const card = (extra: object = {}) => <AskCard permission={{ sessionId: "s1", ask: permission }} {...extra} />;

  it("shows the command in mono as plain text and no text field", () => {
    const { container } = render(<AskCard permission={{ sessionId: "s1", ask: { ...permission, summary: "<b>x</b>" } }} />);
    expect(screen.getByText("Bash")).toBeTruthy();
    expect(container.querySelector("pre.ac-cmd")?.textContent).toBe("<b>x</b>");
    expect(container.querySelector("input, textarea")).toBeNull();
  });

  it("allows once", async () => {
    const spy = vi.spyOn(api, "answerPermission").mockResolvedValue(null);
    const done = vi.fn();
    render(card({ onDone: done }));
    fireEvent.click(screen.getByText("Permitir una vez"));
    await waitFor(() => expect(done).toHaveBeenCalled());
    expect(spy).toHaveBeenCalledWith("s1", "p1", "allow_once", undefined);
  });

  it("offers allow always with the rule shown, only when there is a rule", async () => {
    const spy = vi.spyOn(api, "answerPermission").mockResolvedValue(null);
    const { unmount } = render(card());
    expect(screen.getByText("Bash(npm test)")).toBeTruthy();
    fireEvent.click(screen.getByText("Permitir siempre en esta tarea"));
    await waitFor(() => expect(spy).toHaveBeenCalledWith("s1", "p1", "allow_always", undefined));
    unmount();
    render(<AskCard permission={{ sessionId: "s1", ask: { ...permission, rule: null } }} />);
    expect(screen.queryByText("Permitir siempre en esta tarea")).toBeNull();
    expect(screen.getByText("Permitir una vez")).toBeTruthy();
    expect(screen.getByText("Denegar")).toBeTruthy();
  });

  it("denies", async () => {
    const spy = vi.spyOn(api, "answerPermission").mockResolvedValue(null);
    render(card());
    fireEvent.click(screen.getByText("Denegar"));
    await waitFor(() => expect(spy).toHaveBeenCalledWith("s1", "p1", "deny", ""));
  });

  it("shows the error inline and re-enables the buttons", async () => {
    vi.spyOn(api, "answerPermission").mockRejectedValue(new ApiError("no rule to remember for this request", 400));
    const done = vi.fn();
    render(card({ onDone: done }));
    fireEvent.click(screen.getByText("Permitir siempre en esta tarea"));
    expect(await screen.findByText("no rule to remember for this request")).toBeTruthy();
    expect(done).not.toHaveBeenCalled();
    expect(btn("Denegar").disabled).toBe(false);
  });

  it("keys 1-3 follow the buttons", async () => {
    const spy = vi.spyOn(api, "answerPermission").mockResolvedValue(null);
    render(card({ keys: true }));
    fireEvent.keyDown(document.body, { key: "3" });
    await waitFor(() => expect(spy).toHaveBeenCalledWith("s1", "p1", "deny", ""));
  });

  it("an ended card keeps the command and says how it ended, without buttons", () => {
    const ended = (e: object) => ({ sessionId: "s1", ask: permission, ended: e as never });
    const { rerender, container } = render(<AskCard permission={ended({ outcome: "expired" })} />);
    expect(screen.getByText("Caducada · se respondió en la terminal")).toBeTruthy();
    expect(container.querySelector("button")).toBeNull();
    expect(screen.getByText("npm test")).toBeTruthy();
    rerender(<AskCard permission={ended({ outcome: "answered", decision: "allow_always" })} />);
    expect(screen.getByText("Permitida siempre en esta tarea")).toBeTruthy();
    rerender(<AskCard permission={ended({ outcome: "answered", decision: "deny" })} />);
    expect(screen.getByText("Denegada")).toBeTruthy();
  });
});

describe("AskCard: Aviso", () => {
  it("shows its action and Quitar", () => {
    const action = vi.fn();
    const dismiss = vi.fn();
    render(<AskCard notice={{ text: "PR abierta", action: { label: "Abrir PR", onClick: action }, onDismiss: dismiss }} />);
    fireEvent.click(screen.getByText("Abrir PR"));
    fireEvent.click(screen.getByText("Quitar"));
    expect(action).toHaveBeenCalled();
    expect(dismiss).toHaveBeenCalled();
  });

  it("has no Quitar when it is blocking (no onDismiss)", () => {
    render(<AskCard notice={{ text: "Algo", action: { label: "Abrir la tarea", onClick: () => {} } }} />);
    expect(screen.queryByText("Quitar")).toBeNull();
  });

  it("⌫ removes it outside text fields", () => {
    const dismiss = vi.fn();
    render(<AskCard keys notice={{ text: "x", onDismiss: dismiss }} />);
    fireEvent.keyDown(document.body, { key: "Backspace" });
    expect(dismiss).toHaveBeenCalledTimes(1);
  });
});

describe("AskCard: shared", () => {
  it("navigates between several pending asks with «1 de N»", () => {
    const go = vi.fn();
    render(<AskCard ask={single} nav={{ index: 0, total: 3, onChange: go }} />);
    expect(screen.getByText("1 de 3")).toBeTruthy();
    expect((screen.getByLabelText("Anterior") as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByLabelText("Siguiente"));
    expect(go).toHaveBeenCalledWith(1);
  });

  it("has no navigation for a single ask and carries its variant", () => {
    const { container } = render(<AskCard ask={single} variant="chat" />);
    expect(screen.queryByText(/ de /)).toBeNull();
    expect(container.querySelector(".ac-chat")).toBeTruthy();
  });

  it("translates", () => {
    setLanguage("en");
    render(<AskCard permission={{ sessionId: "s1", ask: permission }} />);
    expect(screen.getByText("Allow once")).toBeTruthy();
  });
});
