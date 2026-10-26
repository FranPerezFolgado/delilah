// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { api, ApiError, type Handoff, type InboxItem, type Task } from "../api";
import { setLanguage } from "../i18n";
import * as platform from "../platform";
import { InboxView } from "./InboxView";
import { ToastHost, clearToasts } from "./ui";

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
    const { queryByText } = show({ to: "implement", stage: "saving" });
    await waitFor(() => expect(document.querySelector(".ac-text")?.textContent).toMatch(/sesión nueva/));
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

describe("InboxView permission items", () => {
  const perm: InboxItem = {
    ...item,
    kind: "permission",
    session: { id: "s1", label: "main", primary: true, live: true },
    text: "Bash: npm test",
    pr: undefined,
    permission: { id: "p1", tool: "Bash", summary: "npm test", suggestion: null, rule: null },
  };

  it("renders the card, without allow-always when there is no rule, and answers", async () => {
    const answer = vi.spyOn(api, "answerPermission").mockResolvedValue(null);
    const onChanged = vi.fn();
    render(<InboxView items={[perm]} onOpenTask={() => {}} onOpenChanges={() => {}} onChanged={onChanged} />);
    expect(screen.queryByText("Always allow in this task")).toBeNull();
    fireEvent.click(screen.getByText("Deny"));
    await waitFor(() => expect(onChanged).toHaveBeenCalled());
    expect(answer).toHaveBeenCalledWith("s1", "p1", "deny", "");
  });

  it("offers no terminal button for a chat-mode ask, but does for a terminal one", () => {
    const { unmount } = render(
      <InboxView items={[perm]} onOpenTask={() => {}} onOpenChanges={() => {}} onChanged={() => {}} />,
    );
    expect(screen.queryByText("Answer in its terminal")).toBeNull();
    unmount();
    render(
      <InboxView items={[{ ...perm, permission: undefined }]} onOpenTask={() => {}} onOpenChanges={() => {}} onChanged={() => {}} />,
    );
    expect(screen.getByText("Answer in its terminal")).toBeTruthy();
  });

  it("shows the rule next to allow always", () => {
    const withRule = { ...perm, permission: { ...perm.permission!, rule: "Bash(npm test)" } };
    render(<InboxView items={[withRule]} onOpenTask={() => {}} onOpenChanges={() => {}} onChanged={() => {}} />);
    expect(screen.getByText("Always allow in this task")).toBeTruthy();
    expect(screen.getByText("Bash(npm test)")).toBeTruthy();
  });
});

describe("InboxView base-stale", () => {
  const stale: InboxItem = {
    kind: "base-stale",
    task: "PROJ-1",
    title: "Reintentos",
    session: null,
    text: "No se pudo actualizar r (sin red); se usó main",
    since: null,
    notice: 9,
  };
  const viewStale = (onChanged = () => {}) =>
    render(<InboxView items={[stale]} onOpenTask={() => {}} onOpenChanges={() => {}} onChanged={onChanged} />);

  it("shows the label and the text, with no PR actions", () => {
    viewStale();
    expect(screen.getByText("Branch may be behind")).toBeTruthy();
    expect(document.querySelector(".ac-text")?.textContent).toBe("No se pudo actualizar r (sin red); se usó main");
    expect(screen.queryByText("View PR")).toBeNull();
    expect(screen.queryByText("Ask the agent to fix it")).toBeNull();
  });

  it("dismisses the notice by its id", async () => {
    const dis = vi.spyOn(api, "dismissNotice").mockResolvedValue(null);
    const onChanged = vi.fn();
    viewStale(onChanged);
    fireEvent.click(screen.getByText("Discard"));
    await waitFor(() => expect(onChanged).toHaveBeenCalled());
    expect(dis).toHaveBeenCalledWith(9);
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
    expect(document.querySelector(".ac-text")?.textContent).toBe("The review keeps failing");
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

describe("InboxView list and detail", () => {
  const notice = (n: number, extra: Partial<InboxItem> = {}): InboxItem => ({
    kind: "turn-done",
    task: "PROJ-1",
    title: "Reintentos",
    session: null,
    text: `Done ${n}`,
    since: null,
    id: `turn-done:PROJ-1:s${n}:${n}`,
    ...extra,
  });
  const renderView = (items: InboxItem[], onChanged = () => {}) =>
    render(<InboxView items={items} onOpenTask={() => {}} onOpenChanges={() => {}} onChanged={onChanged} />);

  it("lists by task in a listbox and shows the count and the selected detail", () => {
    renderView([notice(1), notice(2), notice(3, { task: "PROJ-2", title: "Otra" })]);
    expect(screen.getByRole("listbox")).toBeTruthy();
    expect(screen.getAllByRole("option").length).toBe(3);
    expect(screen.getByText("1 of 2")).toBeTruthy();
    fireEvent.click(screen.getAllByRole("option")[2]);
    expect(screen.queryByText("1 of 2")).toBeNull();
    expect(screen.getAllByText("Done 3").length).toBeGreaterThan(1);
  });

  it("moves the selection with the arrow keys", () => {
    renderView([notice(1), notice(2)]);
    fireEvent.keyDown(document.body, { key: "ArrowDown" });
    expect(screen.getAllByRole("option")[1].getAttribute("aria-selected")).toBe("true");
  });

  it("dismisses a notice with Quitar", async () => {
    const dis = vi.spyOn(api, "dismissInbox").mockResolvedValue(null);
    const onChanged = vi.fn();
    renderView([notice(1)], onChanged);
    fireEvent.click(screen.getByText("Remove"));
    await waitFor(() => expect(onChanged).toHaveBeenCalled());
    expect(dis).toHaveBeenCalledWith("turn-done:PROJ-1:s1:1");
  });

  it("offers no Quitar on a blocking notice", () => {
    renderView([{ ...item, id: "pr-failed:PROJ-1:7:1" }]);
    expect(screen.queryByText("Remove")).toBeNull();
  });

  it("clears the notices and undoes it from the toast", async () => {
    clearToasts();
    vi.spyOn(api, "clearInbox").mockResolvedValue({ ids: ["a", "b"] });
    const restore = vi.spyOn(api, "restoreInbox").mockResolvedValue(null);
    render(<><InboxView items={[notice(1)]} onOpenTask={() => {}} onOpenChanges={() => {}} onChanged={() => {}} /><ToastHost /></>);
    fireEvent.click(screen.getByText("Clear notices"));
    fireEvent.click(await screen.findByText("Undo"));
    await waitFor(() => expect(restore).toHaveBeenCalledWith(["a", "b"]));
    clearToasts();
  });

  it("disables Limpiar avisos when only blocking items remain", () => {
    renderView([{ ...item, id: "pr-failed:PROJ-1:7:1" }]);
    expect((screen.getByText("Clear notices") as HTMLButtonElement).disabled).toBe(true);
  });

  it("shows the empty state with the latest activity", async () => {
    vi.spyOn(api, "sessions").mockResolvedValue([
      { id: "s", task: "PROJ-9", last_activity_at: new Date().toISOString(), last_message: "ran tests" } as never,
    ]);
    renderView([]);
    expect(screen.getByText("Nothing is waiting for you")).toBeTruthy();
    expect(await screen.findByText(/Last activity: PROJ-9 · ran tests · just now/)).toBeTruthy();
  });
});
