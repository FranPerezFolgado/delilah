// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { api, type Handoff, type Session, type Step, type Task } from "../api";
import { setLanguage } from "../i18n";
import { SessionChain } from "./SessionChain";

const task = (handoff: Handoff | null = null): Task => ({
  key: "PROJ-1",
  title: "T",
  project: "web",
  workflow: "feature",
  phase: null,
  saving_mode: true, title_source: "initial", previous_title: null, branch_error: null,
  handoff,
  status: { type: "running" },
});
const sess = (id: string, step: Step, primary: boolean, live = true): Session => ({
  id,
  task: "PROJ-1",
  kind: "claude",
  label: id,
  primary,
  closed: false,
  live,
  read_only: false,
  conversation: false,
  state: "working",
  step,
});

beforeEach(() => setLanguage("es"));
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("SessionChain", () => {
  it("renders nothing without saving mode", () => {
    const { container } = render(
      <SessionChain task={{ ...task(), saving_mode: false }} sessions={[]} onOpenSession={() => {}} onChanged={() => {}} />,
    );
    expect(container.firstChild).toBeNull();
  });

  it("labels each step with its state", () => {
    const { getByLabelText } = render(
      <SessionChain task={task()} sessions={[sess("a", "plan", false), sess("b", "implement", true)]} onOpenSession={() => {}} onChanged={() => {}} />,
    );
    expect(getByLabelText("Spec y plan, terminada")).toBeTruthy();
    expect(getByLabelText("Implementación, en curso")).toBeTruthy();
    expect(getByLabelText("Revisión, pendiente")).toBeTruthy();
  });

  it("opens a done step's tab and the active one's", () => {
    const open = vi.fn();
    const { getByLabelText } = render(
      <SessionChain task={task()} sessions={[sess("a", "plan", false), sess("b", "implement", true)]} onOpenSession={open} onChanged={() => {}} />,
    );
    fireEvent.click(getByLabelText("Spec y plan, terminada").querySelector("button")!);
    expect(open).toHaveBeenLastCalledWith("a");
    fireEvent.click(getByLabelText("Implementación, en curso").querySelector("button")!);
    expect(open).toHaveBeenLastCalledWith("b");
  });

  it("reopens a stopped done session before showing it", async () => {
    const open = vi.fn();
    const reopen = vi.spyOn(api, "reopenSession").mockResolvedValue(sess("a", "plan", false));
    const { getByText } = render(
      <SessionChain task={task()} sessions={[sess("a", "plan", false, false), sess("b", "implement", true)]} onOpenSession={open} onChanged={() => {}} />,
    );
    fireEvent.click(getByText("Reabrir"));
    await waitFor(() => expect(open).toHaveBeenCalledWith("a"));
    expect(reopen).toHaveBeenCalledWith("a");
  });

  it("offers the handoff in the waiting step", () => {
    const { getByText, getByLabelText } = render(
      <SessionChain task={task({ to: "implement", stage: "offered" })} sessions={[sess("a", "plan", true)]} onOpenSession={() => {}} onChanged={() => {}} />,
    );
    expect(getByLabelText("Implementación, esperando")).toBeTruthy();
    expect(getByText("Implementar en sesión nueva")).toBeTruthy();
    expect(getByText("Seguir aquí")).toBeTruthy();
  });

  it("shows the saving state without buttons", () => {
    const { getByText, queryByRole } = render(
      <SessionChain task={task({ to: "implement", stage: "saving" })} sessions={[sess("a", "plan", true)]} onOpenSession={() => {}} onChanged={() => {}} />,
    );
    expect(getByText("Guardando las notas de la sesión…")).toBeTruthy();
    expect(queryByRole("button", { name: "Seguir aquí" })).toBeNull();
  });
});
