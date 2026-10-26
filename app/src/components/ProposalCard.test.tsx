// @vitest-environment jsdom
import { act } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { api } from "../api";
import { setLanguage } from "../i18n";
import { clearToasts, ToastHost } from "./ui";
import { setToastsPaused } from "./ui/Toast";
import { ProposalCard } from "./ProposalCard";
import { inToasts } from "../test/toasts";

beforeEach(() => setLanguage("es"));

afterEach(() => {
  cleanup();
  clearToasts();
  vi.useRealTimers();
  vi.restoreAllMocks();
});

const phased = { workflow: { kind: "phased" as const, plan: "/p", implement: "/i", review: "/r" } };

const proposal = {
  id: "a2",
  questions: [{ question: "¿Creo la tarea?" }],
  proposal: { title: "Reintentos", brief: "El **brief** completo" },
};

describe("ProposalCard", () => {
  it("renders title, open brief and four buttons", () => {
    const { getByRole, getByText } = render(<ProposalCard ask={proposal} />);
    expect(getByRole("heading").textContent).toBe("Reintentos");
    expect(getByText("brief").tagName).toBe("STRONG");
    for (const b of ["Crear", "Cambiar algo", "Descartar"]) expect(getByText(b)).toBeTruthy();
  });

  it("offers autopilot only when the project is phased", () => {
    const none = render(<ProposalCard ask={proposal} />);
    expect(none.queryByText("Crear con autopiloto")).toBeNull();
    none.unmount();
    const single = render(<ProposalCard ask={proposal} projectSettings={{ workflow: { kind: "single", start: { type: "builtin" } } }} />);
    expect(single.queryByText("Crear con autopiloto")).toBeNull();
    single.unmount();
    const yes = render(<ProposalCard ask={proposal} projectSettings={phased} />);
    expect(yes.getByText("Crear con autopiloto")).toBeTruthy();
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

  it("Crear con autopiloto asks first, then answers Crear with launch autopilot and links the task", async () => {
    const answer = vi.spyOn(api, "answerAsk").mockResolvedValue({ task: "LIBRE-8" });
    const { getByText, findByText } = render(<ProposalCard ask={proposal} projectSettings={phased} />);
    fireEvent.click(getByText("Crear con autopiloto"));
    expect(answer).not.toHaveBeenCalled();
    fireEvent.click(getByText("Iniciar"));
    const link = await findByText("LIBRE-8");
    expect(answer).toHaveBeenCalledWith("a2", { "¿Creo la tarea?": "Crear" }, "autopilot");
    expect(link.tagName).toBe("A");
  });

  it("launch error shows the error and still links the backlog task", async () => {
    vi.spyOn(api, "answerAsk").mockResolvedValue({ task: "LIBRE-9", error: "no arrancó" });
    const { getByText, findByText, queryByText } = render(<ProposalCard ask={proposal} projectSettings={phased} />);
    fireEvent.click(getByText("Crear con autopiloto"));
    fireEvent.click(getByText("Iniciar"));
    expect(await findByText("no arrancó")).toBeTruthy();
    expect((await findByText("LIBRE-9")).tagName).toBe("A");
    expect(queryByText("Descartar")).toBeNull();
  });

  it("disables all buttons while busy", async () => {
    vi.spyOn(api, "answerAsk").mockReturnValue(new Promise(() => {}));
    const { getByText } = render(<ProposalCard ask={proposal} projectSettings={phased} />);
    fireEvent.click(getByText("Crear"));
    await waitFor(() => expect((getByText("Crear") as HTMLButtonElement).disabled).toBe(true));
    expect((getByText("Crear con autopiloto") as HTMLButtonElement).disabled).toBe(true);
  });

  it("Cancelar in the autopilot confirmation answers nothing", () => {
    const answer = vi.spyOn(api, "answerAsk").mockResolvedValue({ task: null });
    const { getByText, queryByText } = render(<ProposalCard ask={proposal} projectSettings={phased} />);
    fireEvent.click(getByText("Crear con autopiloto"));
    fireEvent.click(getByText("Cancelar"));
    expect(queryByText("Iniciar")).toBeNull();
    expect(answer).not.toHaveBeenCalled();
  });

  it("Descartar hides the card, sends after the undo window and Deshacer brings it back", async () => {
    vi.useFakeTimers();
    const answer = vi.spyOn(api, "answerAsk").mockResolvedValue({ task: null });
    const onDone = vi.fn();
    const { getByText, queryByRole } = render(
      <>
        <ProposalCard ask={proposal} onDone={onDone} />
        <ToastHost />
      </>,
    );
    fireEvent.click(getByText("Descartar"));
    expect(queryByRole("heading")).toBeNull();
    expect(answer).not.toHaveBeenCalled();
    fireEvent.click(document.body.querySelector(".toast button")!);
    expect(queryByRole("heading")).not.toBeNull();
    await act(async () => void vi.advanceTimersByTime(7000));
    expect(answer).not.toHaveBeenCalled();

    fireEvent.click(getByText("Descartar"));
    await act(async () => void vi.advanceTimersByTime(7000));
    expect(answer).toHaveBeenCalledWith("a2", { "¿Creo la tarea?": "Descartar" });
    expect(onDone).toHaveBeenCalled();
  });

  it("hovering the toast stack past the undo window still sends Descartar and drops the toast", async () => {
    vi.useFakeTimers();
    const answer = vi.spyOn(api, "answerAsk").mockResolvedValue({ task: null });
    const { getByText } = render(
      <>
        <ProposalCard ask={proposal} />
        <ToastHost />
      </>,
    );
    fireEvent.click(getByText("Descartar"));
    expect(inToasts().queryByText("Tarea descartada")).not.toBeNull();
    act(() => setToastsPaused(true));
    fireEvent.mouseEnter(document.body.querySelector(".toast-host")!);
    await act(async () => void vi.advanceTimersByTime(7000));
    expect(answer).toHaveBeenCalledWith("a2", { "¿Creo la tarea?": "Descartar" });
    expect(inToasts().queryByText("Tarea descartada")).toBeNull();
    act(() => setToastsPaused(false));
  });

  it("unmounting with a discard pending sends Descartar and drops the toast", () => {
  const answer = vi.spyOn(api, "answerAsk").mockResolvedValue({ task: null });
  const ui = (card: boolean) => (
    <>
      {card && <ProposalCard ask={proposal} />}
      <ToastHost />
    </>
  );
  const { getByText, rerender } = render(ui(true));
  fireEvent.click(getByText("Descartar"));
  expect(inToasts().queryByText("Tarea descartada")).not.toBeNull();
  rerender(ui(false));
  expect(answer).toHaveBeenCalledWith("a2", { "¿Creo la tarea?": "Descartar" });
  expect(inToasts().queryByText("Tarea descartada")).toBeNull();
});

it.each(["Cambiar algo"])("%s answers with it, creates nothing and shows no error", async (label) => {
    const answer = vi.spyOn(api, "answerAsk").mockResolvedValue({ task: null });
    const onDone = vi.fn();
    const { getByText, container } = render(<ProposalCard ask={proposal} onDone={onDone} />);
    fireEvent.click(getByText(label));
    await waitFor(() => expect(onDone).toHaveBeenCalled());
    expect(answer).toHaveBeenCalledWith("a2", { "¿Creo la tarea?": label });
    expect(container.querySelector(".pc-created")).toBeNull();
    expect(container.querySelector(".pc-error")).toBeNull();
  });
});
