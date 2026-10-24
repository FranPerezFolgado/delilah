// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { api, ApiError, type Handoff, type InboxItem, type Task } from "../api";
import { setLanguage } from "../i18n";
import * as platform from "../platform";
import { InboxView } from "./InboxView";

const item: InboxItem = {
  kind: "pr-failed",
  task: "PROJ-1",
  title: "Reintentos",
  session: null,
  text: "CI falló: test",
  since: null,
  pr: { notice: 7, url: "https://github.com/o/r/pull/3", repo: "r" },
};

const view = (onChanged = () => {}) =>
  render(<InboxView items={[item]} onOpenTask={() => {}} onOpenChanges={() => {}} onChanged={onChanged} />);

beforeEach(() => setLanguage("en"));
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  localStorage.clear();
});

describe("InboxView PR items", () => {
  it("forwards the notice to the agent", async () => {
    const fwd = vi.spyOn(api, "forwardNotice").mockResolvedValue(null);
    const onChanged = vi.fn();
    view(onChanged);
    fireEvent.click(screen.getByText("Ask the agent to fix it"));
    await waitFor(() => expect(onChanged).toHaveBeenCalled());
    expect(fwd).toHaveBeenCalledWith(7);
  });

  it("shows the daemon's message and disables the button on 409", async () => {
    vi.spyOn(api, "forwardNotice").mockRejectedValue(
      new ApiError("paused until 14:30", 409, false, undefined, "2026-10-21T14:30:00Z"),
    );
    view();
    fireEvent.click(screen.getByText("Ask the agent to fix it"));
    await waitFor(() => expect(screen.getByText("paused until 14:30")).toBeTruthy());
    expect((screen.getByText("Ask the agent to fix it") as HTMLButtonElement).disabled).toBe(true);
  });

  it("keeps the button enabled on a 409 that is not a pause", async () => {
    vi.spyOn(api, "forwardNotice").mockRejectedValue(new ApiError("no agent session", 409));
    view();
    fireEvent.click(screen.getByText("Ask the agent to fix it"));
    await waitFor(() => expect(screen.getByText("no agent session")).toBeTruthy());
    expect((screen.getByText("Ask the agent to fix it") as HTMLButtonElement).disabled).toBe(false);
  });

  it("dismisses the notice", async () => {
    const dis = vi.spyOn(api, "dismissNotice").mockResolvedValue(null);
    view();
    fireEvent.click(screen.getByText("Discard"));
    await waitFor(() => expect(dis).toHaveBeenCalledWith(7));
  });

  it("opens the PR externally", () => {
    const open = vi.spyOn(platform, "openExternal").mockResolvedValue();
    view();
    fireEvent.click(screen.getByText("View PR"));
    expect(open).toHaveBeenCalledWith("https://github.com/o/r/pull/3");
  });
});

describe("InboxView pr-blocked", () => {
  const blocked: InboxItem = { ...item, kind: "pr-blocked", text: "Hay cambios sin subir" };
  const viewBlocked = (onChanged = () => {}) =>
    render(<InboxView items={[blocked]} onOpenTask={() => {}} onOpenChanges={() => {}} onChanged={onChanged} />);

  it("archives anyway after confirming, then dismisses the notice", async () => {
    const arch = vi.spyOn(api, "archive").mockResolvedValue({} as never);
    const dis = vi.spyOn(api, "dismissNotice").mockResolvedValue(null);
    viewBlocked();
    fireEvent.click(screen.getByText("Archive anyway"));
    expect(arch).not.toHaveBeenCalled();
    expect(screen.getByText(/workspace and any remaining local branches will be removed/)).toBeTruthy();
    fireEvent.click(screen.getByText("Yes, archive"));
    await waitFor(() => expect(dis).toHaveBeenCalledWith(7));
    expect(arch).toHaveBeenCalledWith("PROJ-1", true);
  });

  it("shows the error and keeps the notice when archiving fails", async () => {
    vi.spyOn(api, "archive").mockRejectedValue(new ApiError("boom", 500));
    const dis = vi.spyOn(api, "dismissNotice").mockResolvedValue(null);
    viewBlocked();
    fireEvent.click(screen.getByText("Archive anyway"));
    fireEvent.click(screen.getByText("Yes, archive"));
    await waitFor(() => expect(screen.getByText("boom")).toBeTruthy());
    expect(dis).not.toHaveBeenCalled();
  });

  it("dismisses the notice", async () => {
    const dis = vi.spyOn(api, "dismissNotice").mockResolvedValue(null);
    viewBlocked();
    fireEvent.click(screen.getByText("Discard"));
    await waitFor(() => expect(dis).toHaveBeenCalledWith(7));
  });
});

const handoffItem: InboxItem = {
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
  saving_mode: true, title_source: "initial", previous_title: null, branch_error: null,
  handoff,
});

const show = (handoff: Handoff | null, onChanged = vi.fn()) => {
  vi.spyOn(api, "tasks").mockResolvedValue([taskWith(handoff)]);
  return render(<InboxView items={[handoffItem]} onOpenTask={() => {}} onOpenChanges={() => {}} onChanged={onChanged} />);
};

describe("InboxView handoff item", () => {
  beforeEach(() => setLanguage("es"));

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

describe("InboxView proposal item", () => {
  it("links the created task and opens it", async () => {
    vi.spyOn(api, "answerAsk").mockResolvedValue({ task: "LIBRE-9" });
    const onOpenTask = vi.fn();
    const proposal: InboxItem = {
      ...item,
      kind: "question",
      ask: { id: "a9", questions: [{ question: "¿Creo la tarea?", options: [] }], proposal: { title: "Idea", brief: "brief" } },
    };
    render(<InboxView items={[proposal]} onOpenTask={onOpenTask} onOpenChanges={() => {}} onChanged={() => {}} />);
    fireEvent.click(screen.getByText("Create"));
    const link = await screen.findByText("LIBRE-9");
    fireEvent.click(link);
    expect(onOpenTask).toHaveBeenCalledWith("LIBRE-9");
  });
});

describe("InboxView branch item", () => {
  const branch: InboxItem = { ...item, kind: "branch", text: "no remote", pr: undefined };

  it("shows the error and retries the branch", async () => {
    const retry = vi.spyOn(api, "retryBranch").mockResolvedValue({} as Task);
    const onChanged = vi.fn();
    render(<InboxView items={[branch]} onOpenTask={() => {}} onOpenChanges={() => {}} onChanged={onChanged} />);
    expect(screen.getByText("The branch could not be created: no remote")).toBeTruthy();
    fireEvent.click(screen.getByText("Retry"));
    await waitFor(() => expect(onChanged).toHaveBeenCalled());
    expect(retry).toHaveBeenCalledWith("PROJ-1");
  });

  it.each([404, 409])("refreshes the list when the retry gets a %i, and keeps the error", async (status) => {
    vi.spyOn(api, "retryBranch").mockRejectedValue(new ApiError("the task is on its branch already", status));
    const onChanged = vi.fn();
    render(<InboxView items={[branch]} onOpenTask={() => {}} onOpenChanges={() => {}} onChanged={onChanged} />);
    fireEvent.click(screen.getByText("Retry"));
    await waitFor(() => expect(onChanged).toHaveBeenCalledTimes(1));
    expect(screen.getByText("the task is on its branch already")).toBeTruthy();
  });

  it("does not refresh on other failures of the retry", async () => {
    vi.spyOn(api, "retryBranch").mockRejectedValue(new ApiError("boom", 500));
    const onChanged = vi.fn();
    render(<InboxView items={[branch]} onOpenTask={() => {}} onOpenChanges={() => {}} onChanged={onChanged} />);
    fireEvent.click(screen.getByText("Retry"));
    await waitFor(() => expect(screen.getByText("boom")).toBeTruthy());
    expect(onChanged).not.toHaveBeenCalled();
  });
});

describe("InboxView autopilot items", () => {
  const ap: InboxItem = { ...item, kind: "autopilot", text: "The review keeps failing", pr: undefined };
  const viewAp = (onChanged = () => {}) =>
    render(<InboxView items={[ap]} onOpenTask={() => {}} onOpenChanges={() => {}} onChanged={onChanged} />);

  it("shows the reason and resumes autopilot", async () => {
    const set = vi.spyOn(api, "setAutopilot").mockResolvedValue({} as Task);
    const onChanged = vi.fn();
    viewAp(onChanged);
    expect(screen.getByText("The review keeps failing")).toBeTruthy();
    fireEvent.click(screen.getByText("Resume autopilot"));
    await waitFor(() => expect(onChanged).toHaveBeenCalled());
    expect(set).toHaveBeenCalledWith("PROJ-1", "resume");
  });

  it("turns autopilot off to carry on by hand", async () => {
    const set = vi.spyOn(api, "setAutopilot").mockResolvedValue({} as Task);
    viewAp();
    fireEvent.click(screen.getByText("Carry on by hand"));
    await waitFor(() => expect(set).toHaveBeenCalledWith("PROJ-1", "off"));
  });
});
