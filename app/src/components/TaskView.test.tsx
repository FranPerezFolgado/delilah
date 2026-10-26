// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { ApiError, type EventRecord, type InboxItem, type Session, type Task } from "../api";
import { setLanguage, t } from "../i18n";
import { stepLabel } from "../chain";
import { clearToasts, ToastHost } from "./ui";
import { TaskView } from "./TaskView";

const events = vi.fn<() => Promise<EventRecord[]>>();
const done = vi.fn();
const reopen = vi.fn();
const resume = vi.fn();
const archive = vi.fn();
const deleteTask = vi.fn();
const fns: Record<string, unknown> = { events, done, reopen, resume, archive, deleteTask };
vi.mock("../api", async (orig) => {
  const mod = await orig<typeof import("../api")>();
  return { ...mod, api: new Proxy({}, { get: (_, k) => fns[k as string] ?? (() => new Promise(() => {})) }) };
});
vi.mock("./SpecPanel", () => ({ SpecPanel: () => null }));

const task: Task = { created_at: "2026-01-01T00:00:00Z", finished_at: null, key: "PROJ-1", title: "A task", project: "web", workflow: "feature", phase: null, status: { type: "queued" }, saving_mode: false, title_source: "initial", previous_title: null, branch_error: null, handoff: null };
const running: Task = { ...task, status: { type: "running" }, branch: "feature/K-1-do-it" };
const session = (over: Partial<Session> = {}): Session => ({
  effective_runtime: "pty", runtime_override: null, last_activity_at: null, last_message: null, id: "s1", task: "PROJ-1", kind: "claude", label: "Claude", primary: true, closed: false, live: true, read_only: false,
  state: "working", conversation: false, step: null, ...over,
});

const noop = () => {};
const show = (tk: Task = task, props: { sessions?: Session[]; asks?: InboxItem[]; onChanged?: () => void; onViewChanges?: () => void; onOpenSession?: (id?: string) => void } = {}) =>
  render(
    <>
      <TaskView
        task={tk}
        sessions={props.sessions ?? []}
        asks={props.asks}
        onOpenSession={props.onOpenSession ?? noop}
        onNewSession={async () => {}}
        onChanged={props.onChanged ?? noop}
        onViewChanges={props.onViewChanges ?? noop}
      />
      <ToastHost />
    </>,
  );
const byText = (c: HTMLElement, sel: string, text: string) =>
  [...c.querySelectorAll<HTMLElement>(sel)].find((e) => e.textContent?.trim() === text);

beforeEach(() => {
  setLanguage("en");
  events.mockResolvedValue([]);
});
afterEach(() => {
  cleanup();
  clearToasts();
  vi.clearAllMocks();
  localStorage.clear();
});

describe("TaskView history", () => {
  it("labels a created-from-conversation event with the conversation label", async () => {
    setLanguage("es");
    events.mockResolvedValue([
      { at: "2026-01-01T10:00:00Z", event: { type: "created-from-conversation", session: "s1", label: "Idea X" } },
    ] as EventRecord[]);
    const { container } = show();
    await waitFor(() => expect(container.querySelector(".tv-history")!.textContent).toContain("Creada desde la conversación «Idea X»"));
  });

  it("shows a relative time and a human sentence per event, newest first", async () => {
    events.mockResolvedValue([
      { at: new Date(Date.now() - 60_000 * 30).toISOString(), event: { type: "start" } },
      { at: new Date().toISOString(), event: { type: "agent-finished" } },
    ] as EventRecord[]);
    const { container } = show();
    await waitFor(() => expect(container.querySelectorAll(".tv-history .tv-ev")).toHaveLength(2));
    const rows = [...container.querySelectorAll(".tv-history .tv-ev")].map((r) => r.textContent);
    expect(rows[0]).toContain("Claude finished");
    expect(rows[0]).toContain("just now");
    expect(rows[1]).toContain("Task created");
    expect(rows[1]).toMatch(/30 min/);
  });
});

describe("step-started history label", () => {
  it("names the step, not a phase", () => {
    setLanguage("es");
    const label = (step: "plan" | "implement" | "review") => t("Paso a {step} (sesión nueva)", { step: stepLabel(step) });
    expect(label("plan")).toBe("Paso a Spec y plan (sesión nueva)");
    expect(label("review")).toBe("Paso a Revisión (sesión nueva)");
  });
});

describe("TaskView meta line", () => {
  it("shows project, workflow and the branch in monospace, with no stepper", () => {
    const { container } = show({ ...running });
    const meta = container.querySelector(".tv-meta")!;
    expect(meta.textContent).toContain("web");
    expect(meta.textContent).toContain("feature");
    const branch = container.querySelector("[data-testid=task-branch]")!;
    expect(branch.textContent).toBe("feature/K-1-do-it");
    expect(branch.className).toContain("mono");
    expect(container.querySelector(".stepper")).toBeNull();
  });

  it("shows no branch when the task has none", () => {
    const { container } = show(task);
    expect(container.querySelector("[data-testid=task-branch]")).toBeNull();
  });
});

describe("mark as done", () => {
  it("marks done in one click and offers Undo, which reopens the task", async () => {
    done.mockResolvedValue({});
    reopen.mockResolvedValue({});
    const onChanged = vi.fn();
    const { container, findByRole } = show(running, { onChanged, sessions: [session()] });
    fireEvent.click(byText(container, "button", "Mark as done")!);
    const toast = await findByRole("status");
    expect(done).toHaveBeenCalledWith("PROJ-1");
    expect(toast.textContent).toContain("Undo");
    await act(async () => {
      fireEvent.click(byText(toast, "button", "Undo")!);
    });
    expect(reopen).toHaveBeenCalledWith("PROJ-1");
    expect(onChanged).toHaveBeenCalledTimes(2);
  });

  it("shows one human error when the reopen is refused", async () => {
    done.mockResolvedValue({});
    reopen.mockRejectedValue(new ApiError("409 conflict", 409));
    const { container, findByRole } = show(running, { sessions: [session()] });
    fireEvent.click(byText(container, "button", "Mark as done")!);
    const toast = await findByRole("status");
    await act(async () => {
      fireEvent.click(byText(toast, "button", "Undo")!);
    });
    await waitFor(() => expect(document.body.textContent).toContain("Could not undo"));
    expect(document.body.textContent).not.toContain("409 conflict");
  });

  it("drops the Undo toast once the workspace is archived", async () => {
    done.mockResolvedValue({});
    archive.mockResolvedValue({});
    const finished: Task = { ...running, status: { type: "done" } };
    const { container, findByRole, rerender } = show(running, { sessions: [session()] });
    fireEvent.click(byText(container, "button", "Mark as done")!);
    await findByRole("status");
    rerender(
      <>
        <TaskView task={finished} sessions={[]} onOpenSession={noop} onNewSession={async () => {}} onChanged={noop} onViewChanges={noop} />
        <ToastHost />
      </>,
    );
    await waitFor(() => expect(document.body.querySelector("[role=status]")).toBeNull());
  });
});

describe("More menu", () => {
  it("lists open changes, archive and delete, and confirms before deleting", async () => {
    deleteTask.mockResolvedValue(null);
    const onViewChanges = vi.fn();
    const finished: Task = { ...running, status: { type: "done" } };
    const { container, findAllByRole, findByRole } = show(finished, { sessions: [session()], onViewChanges });
    fireEvent.click(byText(container, "button", "More")!);
    const items = await findAllByRole("menuitem");
    expect(items.map((i) => i.textContent)).toEqual(["Open changes", "Archive workspace", "Delete task…"]);
    fireEvent.click(items[0]);
    expect(onViewChanges).toHaveBeenCalled();

    fireEvent.click(byText(container, "button", "More")!);
    fireEvent.click((await findAllByRole("menuitem"))[2]);
    const dialog = await findByRole("dialog");
    expect(deleteTask).not.toHaveBeenCalled();
    await act(async () => {
      fireEvent.click(byText(dialog, "button", "Delete task")!);
    });
    expect(deleteTask).toHaveBeenCalledWith("PROJ-1");
  });
});

describe("review notice", () => {
  const reviewing: Task = { ...running, phase: "review" };
  const pending = {
    kind: "question", task: "PROJ-1", title: "t", session: null, text: null, since: null,
    ask: { id: "a", questions: [{ question: "Q?", header: "h", multiSelect: false, options: [{ label: "Yes", description: "" }] }] },
  } as InboxItem;
  const btn = (c: HTMLElement) => byText(c, "button", "Review with Claude")!;

  it("is the primary of the view when no ask is shown", () => {
    const { container } = show(reviewing, { sessions: [session()] });
    expect(btn(container).className).toContain("b-p");
  });

  it("becomes secondary while an inline ask is shown", () => {
    const { container } = show(reviewing, { sessions: [session()], asks: [pending] });
    expect(btn(container).className).toContain("b-s");
  });
});

describe("errors", () => {
  it("shows a human error, never the daemon's raw English message", async () => {
    resume.mockRejectedValue(new ApiError("sqlite: database is locked", 500));
    const { container } = show({ ...running, status: { type: "failed", reason: "x" } }, { sessions: [session({ live: false })] });
    await act(async () => {
      fireEvent.click(byText(container, "button", "Resume")!);
    });
    expect(container.querySelector(".tv-error")!.textContent).toBe("Could not complete the action");
  });
});

describe("pending asks inline", () => {
  const ask = (id: string): InboxItem => ({
    kind: "question", task: "PROJ-1", title: "t", session: null, text: null, since: null,
    ask: { id, questions: [{ question: `Question ${id}`, header: "h", multiSelect: false, options: [{ label: "Yes", description: "" }] }] },
  } as InboxItem);

  it("answers the asks in the tab with 1 of N and never points to the Inbox", () => {
    const { container } = show(running, { asks: [ask("a"), ask("b")] });
    expect(container.textContent).toContain("1 of 2");
    expect(container.textContent).toContain("Question a");
    expect(container.textContent).not.toMatch(/inbox/i);
  });

  it("shows no ask card when nothing is pending", () => {
    const { container } = show(running, { asks: [] });
    expect(container.querySelector(".ac")).toBeNull();
  });
});

describe("sessions", () => {
  it("lists every Claude session of a free workflow with a dot, name, kind and Open", () => {
    const sessions = [
      session(),
      session({ id: "s2", label: "Second", primary: false, state: "waiting" }),
      session({ id: "s3", label: "Terminal", kind: "shell", primary: false }),
    ];
    const onOpenSession = vi.fn();
    const { container } = show({ ...running, workflow: "free" }, { sessions, onOpenSession });
    const rows = container.querySelectorAll(".tv-sessions .tv-srow");
    expect(rows).toHaveLength(3);
    expect(rows[0].textContent).toContain("Main Claude");
    expect(rows[1].textContent).toContain("Second");
    expect(rows[2].textContent).toContain("terminal");
    expect(rows[1].querySelector(".tv-dot-wait")).not.toBeNull();
    fireEvent.click(byText(rows[1] as HTMLElement, "button", "Open")!);
    expect(onOpenSession).toHaveBeenCalledWith("s2");
  });
});

describe("failed task", () => {
  it("shows the reason and Resume", async () => {
    resume.mockResolvedValue({});
    const { container } = show({ ...running, status: { type: "failed", reason: "agent crashed hard" } }, { sessions: [session({ live: false })] });
    expect(container.textContent).toContain("agent crashed hard");
    await act(async () => {
      fireEvent.click(byText(container, "button", "Resume")!);
    });
    expect(resume).toHaveBeenCalledWith("PROJ-1");
  });
});
