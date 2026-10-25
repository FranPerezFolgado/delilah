// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { ApiError, DEFAULT_PROJECT_SETTINGS, type InboxItem, type Project, type Session, type SpecSummary, type Task, type TaskStatus } from "../api";
import { sessionFor, sessionTab } from "../editor/session";
import { setLanguage } from "../i18n";
import { TaskScreen } from "./TaskScreen";

const setSavingMode = vi.hoisted(() => vi.fn(() => new Promise(() => {})));
const dismissNotice = vi.hoisted(() => vi.fn(() => Promise.resolve(null)));
const setAutopilot = vi.hoisted(() => vi.fn(() => new Promise(() => {})));
const updateTask = vi.hoisted(() => vi.fn(() => new Promise(() => {})));
// A spy that returns a rejected promise reports it as unhandled: record the calls apart.
const runtime = vi.hoisted(() => ({ calls: vi.fn(), result: (): Promise<unknown> => Promise.resolve(null) }));
const setRuntime = vi.hoisted(() => (...a: unknown[]) => {
  runtime.calls(...a);
  return runtime.result();
});
const fileApi = vi.hoisted(() => vi.fn(() => new Promise(() => {})));
vi.mock("../api", async (orig) => {
  const mod = await orig<typeof import("../api")>();
  return {
    ...mod,
    api: new Proxy({}, { get: (_, name) => (name === "setSavingMode" ? setSavingMode : name === "setAutopilot" ? setAutopilot : name === "updateTask" ? updateTask : name === "setRuntime" ? setRuntime : name === "file" ? fileApi : name === "dismissNotice" ? dismissNotice : () => new Promise(() => {})) }),
  };
});
vi.mock("./TerminalPane", () => ({ TerminalPane: () => <div data-testid="terminal" /> }));
vi.mock("./ConversationView", async () => {
  const { ToolCard } = await vi.importActual<typeof import("./ToolCard")>("./ToolCard");
  return {
    // A tool entry as the daemon serves it: its path relative to the workspace.
    ConversationView: (p: { onOpenFile: (path: string) => void }) => (
      <div data-testid="conversation">
        <ToolCard
          sessionId="s1"
          onOpenFile={p.onOpenFile}
          entry={{ id: "t1", at: "", kind: "tool", tool: "Edit", tool_use_id: "tu1", summary: "a.rs", path: "src/a.rs", input: {}, result: "", is_error: false, truncated: false }}
        />
      </div>
    ),
  };
});
vi.mock("./FileTree", () => ({ FileTree: () => null }));
vi.mock("./ChangesView", () => ({ ChangesView: () => null }));
vi.mock("./TaskView", () => ({ TaskView: () => null }));
vi.mock("./ProjectView", () => ({ ProjectView: () => null }));
vi.mock("./QuickOpen", () => ({ QuickOpen: () => null }));
vi.mock("./AskCard", () => ({
  AskCard: (p: { ask: { proposal?: { title: string } | null }; onOpenTask?: (k: string) => void }) => (
    <div data-testid="askcard" data-proposal={p.ask.proposal?.title ?? ""}>
      <button onClick={() => p.onOpenTask?.("LIBRE-9")}>open-created</button>
    </div>
  ),
}));

const task = (status: TaskStatus): Task => ({
  key: "PROJ-1",
  title: "A task",
  project: "web",
  workflow: "feature",
  phase: null,
  saving_mode: false,
  handoff: null,
  title_source: "initial",
  previous_title: null,
  branch_error: null,
  status,
});
const spec: SpecSummary = { phase: "plan", dir: "specs/001-x", done: 0, total: 3, current: null };
const project: Project = { name: "web", path: "/w/web", kind: "repo", workflow: "feature", settings: DEFAULT_PROJECT_SETTINGS };

function head(t?: Task, s?: SpecSummary) {
  const { container } = render(
    <TaskScreen
      owner={t ? t.key : "@web"}
      task={t}
      spec={s}
      project={t ? undefined : project}
      sessions={[]}
      asks={[]}
      notices={[]}
      askPosition="right"
      onChanged={() => {}}
    />,
  );
  return container.querySelector(".view-head")!;
}

beforeEach(() => {
  setLanguage("es");
  setSavingMode.mockClear();
  updateTask.mockClear();
  setAutopilot.mockClear();
});
afterEach(() => {
  cleanup();
  localStorage.clear();
});

describe("TaskScreen header phase", () => {
  it("shows the phase pill before the status pill for a running task with a spec", () => {
    const pills = [...head(task({ type: "running" }), spec).querySelectorAll(".pill")];
    expect(pills).toHaveLength(2);
    expect(pills[0].classList.contains("phase-pill")).toBe(true);
    expect(pills[1].classList.contains("phase-pill")).toBe(false);
  });

  it("shows no phase pill for a queued task", () => {
    expect(head(task({ type: "queued" }), spec).querySelector(".phase-pill")).toBeNull();
  });

  it("shows no phase pill for a done task", () => {
    expect(head(task({ type: "done" }), spec).querySelector(".phase-pill")).toBeNull();
  });

  it("marks a chat-mode task as experimental", () => {
    expect(head({ ...task({ type: "running" }), chat_mode: true }).textContent).toContain("experimental: modo chat");
    expect(head(task({ type: "running" })).textContent).not.toContain("experimental: modo chat");
  });

  it("shows no phase pill on the project screen", () => {
    expect(head().querySelector(".phase-pill")).toBeNull();
  });
});

describe("TaskScreen conversation sessions", () => {
  const conv: Session = {
    id: "s1",
    task: "@web",
    kind: "claude",
    label: "Idea",
    primary: false,
    closed: false,
    live: true,
    read_only: false,
    step: null,
    state: "waiting",
    conversation: true,
  };
  const open = (s: Session) => {
    sessionFor("@web").active = sessionTab(s.id);
    return render(
      <TaskScreen owner="@web" project={project} sessions={[s]} asks={[]} notices={[]} askPosition="right" onChanged={() => {}} onConvert={() => {}} />,
    );
  };

  it("shows the conversation view, not the terminal, with no convert bar", () => {
    const { queryByTestId, container } = open(conv);
    expect(queryByTestId("conversation")).not.toBeNull();
    expect(queryByTestId("terminal")).toBeNull();
    expect(container.querySelector(".tterm .ebar")).toBeNull();
  });

  it("opens the file of a tool entry in the editor with its workspace-relative path", () => {
    const { container, getByText } = open(conv);
    fireEvent.click(container.querySelector(".tool-head")!);
    fireEvent.click(getByText("Abrir en el editor"));
    expect(fileApi).toHaveBeenCalledWith("@web", "src/a.rs");
  });

  it("Terminal on a stream session asks for the switch instead of showing a terminal it has not", () => {
    const { queryByTestId, getByRole } = open({ ...conv, runtime: "stream" });
    const btn = getByRole("button", { name: "Terminal" }) as HTMLButtonElement;
    expect(btn.title).toBe("Pasa la tarea a terminal: Claude sigue con la misma conversación");
    fireEvent.click(btn);
    expect(queryByTestId("terminal")).toBeNull();
    expect(queryByTestId("conversation")).not.toBeNull();
  });

  it("toggles to the terminal and back with the Chat / Terminal switch", () => {
    const { queryByTestId, getByRole } = open(conv);
    fireEvent.click(getByRole("button", { name: "Terminal" }));
    expect(queryByTestId("terminal")).not.toBeNull();
    expect(queryByTestId("conversation")).toBeNull();
    fireEvent.click(getByRole("button", { name: "Chat" }));
    expect(queryByTestId("conversation")).not.toBeNull();
    expect(queryByTestId("terminal")).toBeNull();
  });

  it("opens a project session in chat even when it is not an idea conversation", () => {
    const { queryByTestId } = open({ ...conv, conversation: false });
    expect(queryByTestId("conversation")).not.toBeNull();
    expect(queryByTestId("terminal")).toBeNull();
  });

  it("always shows the terminal for a shell session, without the switch", () => {
    const { queryByTestId, queryByRole } = open({ ...conv, kind: "shell", conversation: false });
    expect(queryByTestId("terminal")).not.toBeNull();
    expect(queryByTestId("conversation")).toBeNull();
    expect(queryByRole("button", { name: "Chat" })).toBeNull();
  });

  const inTask = (s: Session, view?: "chat" | "terminal") => {
    sessionFor("PROJ-1").active = sessionTab(s.id);
    return render(
      <TaskScreen owner="PROJ-1" task={{ ...task({ type: "running" }), view }} sessions={[{ ...s, task: "PROJ-1", conversation: false }]} asks={[]} notices={[]} askPosition="right" onChanged={() => {}} />,
    );
  };

  it("starts a task session in chat or terminal from task.view, and the switch overrides it", () => {
    const a = inTask(conv, "chat");
    expect(a.queryByTestId("conversation")).not.toBeNull();
    cleanup();
    const b = inTask(conv, "terminal");
    expect(b.queryByTestId("terminal")).not.toBeNull();
    expect(b.queryByTestId("conversation")).toBeNull();
    fireEvent.click(b.getByRole("button", { name: "Chat" }));
    expect(b.queryByTestId("conversation")).not.toBeNull();
    fireEvent.click(b.getByRole("button", { name: "Terminal" }));
    expect(b.queryByTestId("terminal")).not.toBeNull();
  });

  it("remembers the view picked with the switch as the task's view, with no other view control", () => {
    updateTask.mockClear();
    const r = inTask(conv, "chat");
    expect(r.queryByLabelText("Vista")).toBeNull();
    fireEvent.click(r.getByRole("button", { name: "Terminal" }));
    expect(updateTask).toHaveBeenCalledWith("PROJ-1", { view: "terminal" });
  });

  const pendingAsk = {
    kind: "question",
    task: "@web",
    title: "",
    session: { id: "s1", label: "Idea", primary: false, live: true },
    text: null,
    since: null,
    ask: { id: "a1", questions: [] },
  } as unknown as InboxItem;
  const openWithAsk = (s: Session, ask: InboxItem = pendingAsk, onOpenTask?: (k: string) => void) => {
    sessionFor("@web").active = sessionTab(s.id);
    return render(
      <TaskScreen owner="@web" project={project} sessions={[s]} asks={[ask]} notices={[]} askPosition="right" onChanged={() => {}} onConvert={() => {}} onOpenTask={onOpenTask} />,
    );
  };

  it("has no ask-bar in conversation mode, but keeps it for a terminal session", () => {
    expect(openWithAsk(conv).container.querySelector(".ask-bar")).toBeNull();
    cleanup();
    expect(openWithAsk({ ...conv, kind: "shell", conversation: false }).container.querySelector(".ask-bar")).not.toBeNull();
  });

  it("shows a proposal ask beside the terminal and opens the created task", () => {
    const proposal = {
      ...pendingAsk,
      ask: { id: "a2", questions: [], proposal: { title: "Nueva", brief: "b" } },
    } as unknown as InboxItem;
    const onOpenTask = vi.fn();
    const { getByTestId, getByText, queryByTestId } = openWithAsk({ ...conv, kind: "shell", conversation: false }, proposal, onOpenTask);
    expect(queryByTestId("terminal")).not.toBeNull();
    expect(getByTestId("askcard").getAttribute("data-proposal")).toBe("Nueva");
    fireEvent.click(getByText("open-created"));
    expect(onOpenTask).toHaveBeenCalledWith("LIBRE-9");
  });
});

describe("TaskScreen saving mode", () => {
  const saving = (status: TaskStatus, on = true) => {
    const { container } = render(
      <TaskScreen
        owner="PROJ-1"
        task={{ ...task(status), saving_mode: on }}
        sessions={[]}
        asks={[]}
        notices={[]}
        askPosition="right"
        onChanged={() => {}}
      />,
    );
    return container;
  };

  it("shows the marker and the chain with saving mode", () => {
    const c = saving({ type: "running" });
    expect(c.querySelector(".saving-pill")?.textContent).toContain("Modo ahorro");
    expect(c.querySelector(".chain")).not.toBeNull();
    expect(c.textContent).not.toContain("Activar modo ahorro");
  });

  it("shows neither without it, only the activate action while running", () => {
    const c = saving({ type: "running" }, false);
    expect(c.querySelector(".saving-pill")).toBeNull();
    expect(c.querySelector(".chain")).toBeNull();
    expect(c.textContent).toContain("Activar modo ahorro");
  });

  it("offers no activate action on a done task", () => {
    expect(saving({ type: "done" }, false).textContent).not.toContain("Activar modo ahorro");
  });

  it("turns saving mode off and on while running", () => {
    saving({ type: "running" });
    fireEvent.click(screen.getByText("Desactivar"));
    expect(setSavingMode).toHaveBeenCalledWith("PROJ-1", false);
    cleanup();
    saving({ type: "running" }, false);
    fireEvent.click(screen.getByText("Activar modo ahorro"));
    expect(setSavingMode).toHaveBeenLastCalledWith("PROJ-1", true);
  });
});

describe("TaskScreen autopilot control", () => {
  const withAp = (over: Partial<Task>) => head({ ...task({ type: "running" }), ...over });

  it("offers to turn it on when off, in a spec-workflow task only", () => {
    withAp({});
    fireEvent.click(screen.getByText("Activar autopiloto"));
    expect(setAutopilot).toHaveBeenCalledWith("PROJ-1", "on");
    cleanup();
    withAp({ workflow: "prompt" });
    expect(screen.queryByText("Activar autopiloto")).toBeNull();
    cleanup();
    withAp({ status: { type: "done" } });
    expect(screen.queryByText("Activar autopiloto")).toBeNull();
  });

  it("offers to turn it off while running, and leaves a paused one to the banner", () => {
    withAp({ saving_mode: true, autopilot: { state: "running", since: "2026-10-21T10:00:00Z" } });
    fireEvent.click(screen.getByTitle("Desactiva el autopiloto y sigue a mano"));
    expect(setAutopilot).toHaveBeenCalledWith("PROJ-1", "off");
    cleanup();
    withAp({ saving_mode: true, autopilot: { state: "paused", since: "2026-10-21T10:00:00Z" } });
    expect(screen.queryByTitle("Desactiva el autopiloto y sigue a mano")).toBeNull();
    expect(screen.queryByText("Activar autopiloto")).toBeNull();
  });
});

describe("TaskScreen follows the primary session", () => {
  const sess = (id: string, primary: boolean): Session => ({
    id,
    task: "PROJ-1",
    kind: "claude",
    label: id,
    primary,
    closed: false,
    live: true,
    read_only: false,
    conversation: false,
    state: "working",
    step: "plan",
  });
  const screenOf = (owner: string, sessions: Session[]) => (
    <TaskScreen
      owner={owner}
      task={{ ...task({ type: "running" }), key: owner, saving_mode: true }}
      sessions={sessions}
      asks={[]}
      notices={[]}
      askPosition="right"
      onChanged={() => {}}
    />
  );
  const selected = () => screen.getAllByRole("tab").find((x) => x.getAttribute("aria-selected") === "true")?.textContent;

  it("selects the new primary's tab after a handoff", () => {
    const { rerender } = render(screenOf("PROJ-1", [sess("a", true)]));
    expect(selected()).toBe("Tarea");
    rerender(screenOf("PROJ-1", [sess("a", false), sess("b", true)]));
    expect(selected()).toBe("b");
  });

  it("does not steal the tab when the screen switches to another task", () => {
    const { rerender } = render(screenOf("PROJ-1", [sess("a", true)]));
    rerender(screenOf("PROJ-2", [sess("c", true)]));
    expect(selected()).toBe("Tarea");
  });
});

describe("TaskScreen base-stale banner", () => {
  const stale: InboxItem = {
    kind: "base-stale",
    task: "PROJ-1",
    title: "A task",
    session: null,
    text: "No se pudo actualizar r (sin red); se usó main",
    since: null,
    notice: 9,
  };
  const screenWith = (notices: InboxItem[], onChanged = () => {}) =>
    render(<TaskScreen owner="PROJ-1" task={task({ type: "running" })} sessions={[]} asks={[]} notices={notices} askPosition="right" onChanged={onChanged} />);

  it("shows the notice text and dismisses it", async () => {
    dismissNotice.mockClear();
    const onChanged = vi.fn();
    const { getByText } = screenWith([stale], onChanged);
    expect(getByText("No se pudo actualizar r (sin red); se usó main")).toBeTruthy();
    fireEvent.click(getByText("Descartar"));
    await waitFor(() => expect(onChanged).toHaveBeenCalled());
    expect(dismissNotice).toHaveBeenCalledWith(9);
  });

  it("has no banner without notices", () => {
    const { container } = screenWith([]);
    expect(container.querySelector(".base-stale")).toBeNull();
  });
});

describe("TaskScreen editable title", () => {
  const edit = (t: Task) => {
    const h = head(t);
    fireEvent.click(h.querySelector("h1")!);
    return screen.getByRole("textbox") as HTMLInputElement;
  };

  it("saves the new title with Enter", () => {
    const input = edit(task({ type: "running" }));
    fireEvent.change(input, { target: { value: "Better name" } });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(updateTask).toHaveBeenCalledWith("PROJ-1", { title: "Better name" });
  });

  it("cancels with Escape", () => {
    const input = edit(task({ type: "running" }));
    fireEvent.change(input, { target: { value: "Other" } });
    fireEvent.keyDown(input, { key: "Escape" });
    expect(updateTask).not.toHaveBeenCalled();
    expect(screen.queryByRole("textbox")).toBeNull();
    expect(document.querySelector("h1")!.textContent).toBe("A task");
  });

  it("refuses an empty title", () => {
    const input = edit(task({ type: "running" }));
    fireEvent.change(input, { target: { value: "   " } });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(updateTask).not.toHaveBeenCalled();
  });

  it("offers to restore the previous title", () => {
    const h = head({ ...task({ type: "running" }), title_source: "spec", previous_title: "Old name" });
    expect(h.textContent).toContain("Antes: Old name");
    fireEvent.click(screen.getByText("Restaurar"));
    expect(updateTask).toHaveBeenCalledWith("PROJ-1", { title: "Old name" });
  });

  it("shows no restore without a previous title", () => {
    const h = head(task({ type: "running" }));
    expect(h.textContent).not.toContain("Antes:");
    expect(screen.queryByText("Restaurar")).toBeNull();
  });
});

describe("TaskScreen runtime switch", () => {
  const stream: Session = { id: "s1", task: "PROJ-1", kind: "claude", label: "Main", primary: true, closed: false, live: true, read_only: false, step: null, state: "waiting", conversation: false, runtime: "stream" };
  const screen1 = (t: Task, s: Session) => (
    <TaskScreen owner="PROJ-1" task={t} sessions={[s]} asks={[]} notices={[]} askPosition="right" onChanged={() => {}} />
  );
  const chatTask = { ...task({ type: "running" }), chat_mode: true };

  beforeEach(() => {
    runtime.calls.mockClear();
    runtime.result = () => Promise.resolve(null);
  });

  beforeEach(() => {
    sessionFor("PROJ-1").active = sessionTab("s1");
  });

  it("has no runtime button in the header: Terminal on a chat-mode session switches the task", async () => {
    const r = render(screen1(chatTask, stream));
    expect(r.queryByRole("button", { name: /Pasar a/ })).toBeNull();
    const term = r.getByRole("button", { name: "Terminal" }) as HTMLButtonElement;
    expect(term.disabled).toBe(false);
    expect(term.title).toContain("Pasa la tarea a terminal");
    fireEvent.click(term);
    expect(runtime.calls).toHaveBeenCalledWith("PROJ-1", "terminal");
    await waitFor(() => expect((r.getByRole("button", { name: "Terminal" }) as HTMLButtonElement).disabled).toBe(false));
    r.rerender(screen1({ ...chatTask, chat_mode: false }, { ...stream, runtime: "pty" }));
    // The terminal is what shows, not the chat the session no longer has.
    expect(r.getByRole("button", { name: "Terminal" }).getAttribute("aria-pressed")).toBe("true");
    expect(r.getByRole("button", { name: "Chat" }).getAttribute("aria-pressed")).toBe("false");
  });

  it("Chat on a chat-mode session does not switch anything", () => {
    const r = render(screen1(chatTask, stream));
    fireEvent.click(r.getByRole("button", { name: "Chat" }));
    expect(runtime.calls).not.toHaveBeenCalled();
  });

  it("offers Probar modo chat in the chat view of a PTY task, disabled while it switches", async () => {
    let done!: () => void;
    const pending = new Promise<null>((res) => (done = () => res(null)));
    runtime.result = () => pending;
    const r = render(screen1(task({ type: "running" }), { ...stream, runtime: "pty" }));
    const btn = r.getByRole("button", { name: "Probar modo chat (experimental)" }) as HTMLButtonElement;
    expect(btn.title).toContain("Experimental");
    fireEvent.click(btn);
    expect(runtime.calls).toHaveBeenCalledWith("PROJ-1", "chat");
    expect(btn.disabled).toBe(true);
    done();
    await waitFor(() => expect(btn.disabled).toBe(false));
  });

  it("does not offer Probar modo chat in the terminal view nor on a chat-mode task", () => {
    const a = render(screen1({ ...task({ type: "running" }), view: "terminal" }, { ...stream, runtime: "pty" }));
    expect(a.queryByRole("button", { name: "Probar modo chat (experimental)" })).toBeNull();
    cleanup();
    const b = render(screen1(chatTask, stream));
    expect(b.queryByRole("button", { name: "Probar modo chat (experimental)" })).toBeNull();
  });

  it("shows a refused switch next to the Chat / Terminal switch", async () => {
    runtime.result = () => Promise.reject(new ApiError("wait for the turn to end or press Detener", 409));
    const r = render(screen1(chatTask, stream));
    fireEvent.click(r.getByRole("button", { name: "Terminal" }));
    const alert = await r.findByRole("alert");
    expect(alert.closest(".view-switch-wrap")).not.toBeNull();
    expect(alert.textContent).toContain("Espera a que acabe el turno");
    fireEvent.click(r.getByRole("button", { name: "Cerrar" }));
    expect(r.queryByRole("alert")).toBeNull();
  });
});
