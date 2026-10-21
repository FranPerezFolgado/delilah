// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { api, ApiError, type Handoff, type InboxItem, type Task } from "../api";
import { setLanguage } from "../i18n";
import { InboxView } from "./InboxView";

const item: InboxItem = {
  kind: "handoff",
  task: "PROJ-1",
  title: "Reintentos",
  session: { id: "s-1", label: "Claude", primary: true, live: true },
  text: "Las tareas están listas. ¿Implementar en una sesión nueva?",
  since: null,
};

const taskWith = (handoff: Handoff | null): Task => ({
  key: "PROJ-1",
  title: "Reintentos",
  project: "web",
  workflow: "feature",
  phase: null,
  status: { type: "waiting-for-user" },
  saving_mode: true,
  handoff,
});

const show = (handoff: Handoff | null, onChanged = vi.fn()) => {
  vi.spyOn(api, "tasks").mockResolvedValue([taskWith(handoff)]);
  return render(<InboxView items={[item]} onOpenTask={() => {}} onOpenChanges={() => {}} onChanged={onChanged} />);
};

beforeEach(() => setLanguage("es"));
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  localStorage.clear();
});

describe("InboxView handoff item", () => {
  it("offers to implement in a new session or to stay", async () => {
    const call = vi.spyOn(api, "handoff").mockResolvedValue(taskWith(null));
    const onChanged = vi.fn();
    const { findByText, getByText } = show({ to: "implement", stage: "offered" }, onChanged);
    fireEvent.click(await findByText("Implementar en sesión nueva"));
    await waitFor(() => expect(call).toHaveBeenCalledWith("PROJ-1", "confirm"));
    await waitFor(() => expect(onChanged).toHaveBeenCalled());
    fireEvent.click(getByText("Seguir aquí"));
    await waitFor(() => expect(call).toHaveBeenLastCalledWith("PROJ-1", "decline"));
  });

  it("disables the buttons while the request runs", async () => {
    let finish: (t: Task) => void = () => {};
    vi.spyOn(api, "handoff").mockReturnValue(new Promise((r) => (finish = r)));
    const { findByText, getByText } = show({ to: "implement", stage: "offered" });
    fireEvent.click(await findByText("Implementar en sesión nueva"));
    await waitFor(() => expect((getByText("Seguir aquí") as HTMLButtonElement).disabled).toBe(true));
    expect((getByText("Implementar en sesión nueva") as HTMLButtonElement).disabled).toBe(true);
    finish(taskWith(null));
  });

  it("shows the failure with a retry", async () => {
    const call = vi.spyOn(api, "handoff").mockResolvedValue(taskWith(null));
    const { findByText, queryByText } = show({ to: "implement", stage: "failed", error: "sin red" });
    expect(await findByText(/sin red/)).toBeTruthy();
    expect(queryByText("Seguir aquí")).toBeNull();
    fireEvent.click(await findByText("Reintentar"));
    await waitFor(() => expect(call).toHaveBeenCalledWith("PROJ-1", "retry"));
  });

  it("shows no buttons while saving", async () => {
    const { findByText, queryByText } = show({ to: "implement", stage: "saving" });
    await findByText(/sesión nueva/);
    await Promise.resolve();
    expect(queryByText("Seguir aquí")).toBeNull();
    expect(queryByText("Reintentar")).toBeNull();
  });

  it("shows a 409 instead of crashing", async () => {
    vi.spyOn(api, "handoff").mockRejectedValue(new ApiError("ya empezó", 409));
    const { findByText } = show({ to: "implement", stage: "offered" });
    fireEvent.click(await findByText("Implementar en sesión nueva"));
    expect(await findByText("ya empezó")).toBeTruthy();
  });
});
