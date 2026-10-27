// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { ApiError, DEFAULT_PROJECT_SETTINGS, type InboxItem, type Project, type Session, type SpecSummary, type Task, type TaskStatus } from "../api";
import { OFFICE, sessionFor, sessionTab } from "../editor/session";
import { setLanguage } from "../i18n";
import { TaskScreen } from "./TaskScreen";
import { ToastHost, clearToasts } from "./ui";
import { toastsText } from "../test/toasts";

const setSavingMode = vi.hoisted(() => vi.fn(() => new Promise(() => {})));
const dismissNotice = vi.hoisted(() => vi.fn(() => Promise.resolve(null)));
const baseStatus = vi.hoisted(() => vi.fn((): Promise<unknown> => new Promise(() => {})));
const updateBranch = vi.hoisted(() => vi.fn((): Promise<unknown> => new Promise(() => {})));
const setAutopilot = vi.hoisted(() => vi.fn(() => new Promise(() => {})));
const updateTask = vi.hoisted(() => vi.fn(() => new Promise(() => {})));
// A spy that returns a rejected promise reports it as unhandled: record the calls apart.
const fileApi = vi.hoisted(() => vi.fn(() => new Promise(() => {})));
const openSession = vi.hoisted(() => vi.fn(() => new Promise(() => {})));
const sessionRuntime = vi.hoisted(() => ({ calls: vi.fn(), result: (): Promise<unknown> => Promise.resolve(null) }));
const setSessionRuntime = vi.hoisted(() => (...a: unknown[]) => {
  sessionRuntime.calls(...a);
  return sessionRuntime.result();
});
vi.mock("../api", async (orig) => {
  const mod = await orig<typeof import("../api")>();
  return {
    ...mod,
    api: new Proxy({}, { get: (_, name) => (name === "setSavingMode" ? setSavingMode : name === "setAutopilot" ? setAutopilot : name === "updateTask" ? updateTask : name === "setSessionRuntime" ? setSessionRuntime : name === "openSession" ? openSession : name === "file" ? fileApi : name === "dismissNotice" ? dismissNotice : name === "baseStatus" ? baseStatus : name === "updateBranch" ? updateBranch : () => new Promise(() => {})) }),
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
vi.mock("./TaskView", () => ({
  TaskView: (p: { asks?: unknown[]; onOpenTask?: (k: string) => void }) => (
    <div data-testid="taskview" data-asks={p.asks?.length ?? -1}>
      <button onClick={() => p.onOpenTask?.("LIBRE-7")}>open-task-view</button>
    </div>
  ),
}));
vi.mock("./ProjectView", () => ({ ProjectView: () => <div data-testid="projectview" /> }));
vi.mock("../playground/OfficeTab", () => ({
  OfficeTab: (p: { owner: string; online?: boolean; onOpenSession: (id: string) => void }) => (
    <div data-testid="office" data-owner={p.owner} data-online={String(p.online)}>
      <button onClick={() => p.onOpenSession("s9")}>open-lead</button>
    </div>
  ),
}));
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
  created_at: "2026-01-01T00:00:00Z",
  finished_at: null,
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
  return container.querySelector(".th, .ts-head")!;
}

beforeEach(() => {
  setLanguage("es");
  setSavingMode.mockClear();
  updateTask.mockClear();
  setAutopilot.mockClear();
  setSavingMode.mockClear();
});
afterEach(() => {
  cleanup();
  localStorage.clear();
});

describe("TaskScreen header", () => {
  it("uses the task header: key, title and the phase bars of a running task with a spec", () => {
    const h = head(task({ type: "running" }), spec);
    expect(h.classList.contains("th")).toBe(true);
    expect(h.textContent).toContain("PROJ-1");
    expect(h.querySelector('[aria-label="Cambiar de fase"]')).not.toBeNull();
  });

  it("shows no phase menu for a queued or a done task", () => {
    expect(head(task({ type: "queued" }), spec).querySelector('[aria-label="Cambiar de fase"]')).toBeNull();
    cleanup();
    expect(head(task({ type: "done" }), spec).querySelector('[aria-label="Cambiar de fase"]')).toBeNull();
  });

  it("has no experimental chat-mode pill", () => {
    expect(head({ ...task({ type: "running" }), chat_mode: true }).textContent).not.toContain("experimental");
  });

  it("uses a project header on the project screen", () => {
    const h = head();
    expect(h.classList.contains("ts-head")).toBe(true);
    expect(h.textContent).toContain("web");
    expect(h.querySelector('[aria-label="Cambiar de fase"]')).toBeNull();
  });
});

describe("TaskScreen tabs", () => {
  const sess = (id: string, over: Partial<Session> = {}): Session => ({
    effective_runtime: "pty", runtime_override: null, last_activity_at: null, last_message: null, id, task: "PROJ-1", kind: "claude", label: id, primary: false, closed: false, live: true, read_only: false, conversation: false, state: "working", step: null, ...over,
  });
  const taskScreen = (sessions: Session[], t = task({ type: "running" }), extra: Partial<React.ComponentProps<typeof TaskScreen>> = {}) =>
    render(<TaskScreen owner="PROJ-1" task={t} sessions={sessions} asks={[]} notices={[]} askPosition="right" onChanged={() => {}} {...extra} />);
  const labels = () => screen.getAllByRole("tab").map((x) => x.textContent);

  beforeEach(() => {
    openSession.mockClear();
    localStorage.clear();
    sessionFor("PROJ-1").active = "\0overview";
    sessionFor("PROJ-1").changesOpen = false;
  });

  it("lists Tarea, the primary as Claude principal and the other sessions", () => {
    taskScreen([sess("a", { primary: true }), sess("b", { label: "Review" })]);
    expect(labels()).toEqual(["Tarea", "Oficina", "Claude principal", "Review", "PR"]);
  });

  it.each([["implement", "Implementación"], ["review", "Revisión"], ["plan", "Spec y plan"]])("names the primary tab after its phase (%s)", (step, name) => {
    taskScreen([sess("a", { primary: true, step: step as Session["step"], label: `${name}: Mi tarea` })]);
    expect(labels()).toEqual(["Tarea", "Oficina", name, "PR"]);
  });

  it("keeps the stored label as the phase tab tooltip and does not allow closing it", () => {
    taskScreen([sess("a", { primary: true, step: "implement", label: "Implementación: Mi tarea" })]);
    expect(screen.getAllByRole("tab")[2].getAttribute("title")).toBe("Implementación: Mi tarea");
    expect(screen.queryByRole("button", { name: "Cerrar Implementación" })).toBeNull();
  });

  it("keeps earlier phase sessions under their phase name, in order, and closable", () => {
    taskScreen([sess("old", { step: "implement", live: false, closed: false }), sess("new", { primary: true, step: "review" })]);
    expect(labels()).toEqual(["Tarea", "Oficina", "Implementación", "Revisión", "PR"]);
    expect(screen.queryByRole("button", { name: "Cerrar Implementación" })).not.toBeNull();
    expect(screen.queryByRole("button", { name: "Cerrar Revisión" })).toBeNull();
  });

  it("names a live earlier phase session after its phase when asking to close it", () => {
    taskScreen([sess("old", { step: "implement", label: "Implementación: Mi tarea" }), sess("new", { primary: true, step: "review" })]);
    fireEvent.click(screen.getByRole("button", { name: "Cerrar Implementación" }));
    expect(screen.getByText(/Cerrar «Implementación» detiene ese Claude/)).not.toBeNull();
  });

  it("keeps the own label of an extra Claude session with no step", () => {
    taskScreen([sess("a", { primary: true, step: "review" }), sess("b", { label: "Review", step: null })]);
    expect(labels()).toEqual(["Tarea", "Oficina", "Revisión", "Review", "PR"]);
  });

  it("orders the tabs Tarea, sessions, files, Cambios, PR", () => {
    sessionFor("PROJ-1").changesOpen = true;
    sessionFor("PROJ-1").tabs = [{ path: "src/a.rs", dirty: false } as never];
    taskScreen([sess("a", { primary: true })]);
    expect(labels().map((l) => l?.replace(/\d+$/, ""))).toEqual(["Tarea", "Oficina", "Claude principal", "a.rs", "Cambios", "PR"]);
    sessionFor("PROJ-1").tabs = [];
  });

  it("only non-primary sessions can be closed", () => {
    taskScreen([sess("a", { primary: true }), sess("b", { label: "Review" })]);
    expect(screen.queryByRole("button", { name: "Cerrar Claude principal" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Cerrar Review" })).not.toBeNull();
  });

  it("starts a terminal from the + Sesión menu, and has no + Claude / + Terminal buttons", () => {
    taskScreen([sess("a", { primary: true })]);
    expect(screen.queryByText("+ Claude")).toBeNull();
    expect(screen.queryByText("+ Terminal")).toBeNull();
    fireEvent.click(screen.getByText("+ Sesión"));
    fireEvent.click(screen.getByRole("menuitem", { name: /Terminal/ }));
    expect(openSession).toHaveBeenCalledWith("PROJ-1", expect.objectContaining({ kind: "shell" }));
  });

  it("passes the asks and onOpenTask to the Tarea tab", () => {
    const onOpenTask = vi.fn();
    const ask = { kind: "question", task: "PROJ-1", title: "", session: null, text: null, since: null, ask: { id: "a", questions: [] } } as unknown as InboxItem;
    const r = taskScreen([sess("a", { primary: true })], undefined, { asks: [ask], onOpenTask });
    expect(r.getByTestId("taskview").getAttribute("data-asks")).toBe("1");
    fireEvent.click(r.getByText("open-task-view"));
    expect(onOpenTask).toHaveBeenCalledWith("LIBRE-7");
  });

  it("hides the file tree while the Cambios tab shows and brings it back on the others", () => {
    const s = sessionFor("PROJ-1");
    const r = taskScreen([sess("a", { primary: true })]);
    expect(r.container.querySelector(".tpanel")).not.toBeNull();
    cleanup();
    s.changesOpen = true;
    s.active = "\0changes";
    const c = taskScreen([sess("a", { primary: true })]);
    expect(c.container.querySelector(".tpanel")).toBeNull();
    expect(labels().some((l) => l?.startsWith("Cambios"))).toBe(true);
    fireEvent.click(screen.getByRole("tab", { name: "Tarea" }));
    expect(c.container.querySelector(".tpanel")).not.toBeNull();
  });

  it("shows the Proyecto tab with the project view on the project screen", () => {
    const r = render(<TaskScreen owner="@web" project={project} sessions={[]} asks={[]} notices={[]} askPosition="right" onChanged={() => {}} />);
    expect(screen.getAllByRole("tab")[0].textContent).toBe("Proyecto");
    expect(r.queryByTestId("projectview")).not.toBeNull();
  });

  it("keeps a single inline style (the tree width)", () => {
    const r = taskScreen([sess("a", { primary: true })]);
    expect([...r.container.querySelectorAll("[style]")].filter((e) => !e.closest(".ehost")).map((e) => e.className)).toEqual(["tscreen with-tree"]);
  });
});

describe("TaskScreen conversation sessions", () => {
  const conv: Session = {
    effective_runtime: "pty", runtime_override: null, last_activity_at: null, last_message: null, id: "s1",
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

  it("offers no Chat / Terminal switch on a project conversation: the daemon refuses it", () => {
    const { container, queryByRole } = open(conv);
    expect(container.querySelector(".svs")).toBeNull();
    expect(queryByRole("button", { name: /^Terminal$/ })).toBeNull();
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
    expect(queryByRole("button", { name: /^Chat/ })).toBeNull();
  });

  const inTask = (s: Session, view?: "chat" | "terminal") => {
    sessionFor("PROJ-1").active = sessionTab(s.id);
    return render(
      <TaskScreen owner="PROJ-1" task={{ ...task({ type: "running" }), view }} sessions={[{ ...s, task: "PROJ-1", conversation: false }]} asks={[]} notices={[]} askPosition="right" onChanged={() => {}} />,
    );
  };

  it("shows a task session in the view of its runtime", () => {
    const a = inTask({ ...conv, runtime: "stream" }, "terminal");
    expect(a.queryByTestId("conversation")).not.toBeNull();
    cleanup();
    const b = inTask({ ...conv, runtime: "pty" }, "chat");
    expect(b.queryByTestId("terminal")).not.toBeNull();
    expect(b.queryByTestId("conversation")).toBeNull();
    fireEvent.click(b.getByRole("button", { name: /^Chat/ }));
    expect(b.getByText("Pasar a chat")).toBeTruthy();
    expect(b.queryByTestId("terminal")).not.toBeNull();
  });

  it("Terminal on a stream task session asks to confirm instead of showing a terminal it has not", () => {
    const r = inTask({ ...conv, runtime: "stream" });
    fireEvent.click(r.getByRole("button", { name: /^Terminal$/ }));
    expect(r.queryByText("Pasar a terminal")).not.toBeNull();
    expect(r.queryByTestId("terminal")).toBeNull();
    expect(r.queryByTestId("conversation")).not.toBeNull();
  });

  it("disables the switch with the reason while the session is mid-turn", () => {
    const r = inTask({ ...conv, runtime: "pty", state: "working" });
    const chat = r.getByRole("button", { name: /^Chat/ }) as HTMLButtonElement;
    expect(chat.disabled).toBe(true);
    expect(chat.title).toBe("Espera a que termine el turno");
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
  const ahorro = () => screen.getByRole("button", { name: "Ahorro" });

  it("shows the toggle on and the chain with saving mode, with no pill", () => {
    const c = saving({ type: "running" });
    expect(ahorro().getAttribute("aria-pressed")).toBe("true");
    expect(c.querySelector(".chain")).not.toBeNull();
    expect(c.querySelector(".saving-pill")).toBeNull();
  });

  it("shows the toggle off and no chain without it", () => {
    const c = saving({ type: "running" }, false);
    expect(ahorro().getAttribute("aria-pressed")).toBe("false");
    expect(c.querySelector(".chain")).toBeNull();
  });

  it("offers no toggles on a done task", () => {
    saving({ type: "done" }, false);
    expect(screen.queryByRole("button", { name: "Ahorro" })).toBeNull();
  });

  it("turns saving mode off and on", () => {
    saving({ type: "running" });
    fireEvent.click(ahorro());
    expect(setSavingMode).toHaveBeenCalledWith("PROJ-1", false);
    cleanup();
    saving({ type: "running" }, false);
    fireEvent.click(ahorro());
    expect(setSavingMode).toHaveBeenLastCalledWith("PROJ-1", true);
  });
});

describe("TaskScreen autopilot control", () => {
  const withAp = (over: Partial<Task>) => head({ ...task({ type: "running" }), ...over });
  const since = "2026-10-21T10:00:00Z";

  it("turns it on after confirming", () => {
    withAp({});
    fireEvent.click(screen.getByRole("button", { name: "Autopiloto" }));
    expect(setAutopilot).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Activar" }));
    expect(setAutopilot).toHaveBeenCalledWith("PROJ-1", "on");
  });

  it("turns it off while running, and resumes a paused one", () => {
    withAp({ saving_mode: true, autopilot: { state: "running", since } });
    fireEvent.click(screen.getByRole("button", { name: "Autopiloto" }));
    expect(setAutopilot).toHaveBeenCalledWith("PROJ-1", "off");
    cleanup();
    withAp({ saving_mode: true, autopilot: { state: "paused", since } });
    fireEvent.click(screen.getByRole("button", { name: /en pausa · Reanudar/ }));
    expect(setAutopilot).toHaveBeenLastCalledWith("PROJ-1", "resume");
  });

  it("has no autopilot controls on a done task", () => {
    withAp({ status: { type: "done" } });
    expect(screen.queryByRole("button", { name: "Autopiloto" })).toBeNull();
  });
});

describe("TaskScreen follows the primary session", () => {
  const sess = (id: string, primary: boolean, step: Session["step"] = "plan"): Session => ({
    effective_runtime: "pty", runtime_override: null, last_activity_at: null, last_message: null, id,
    task: "PROJ-1",
    kind: "claude",
    label: id,
    primary,
    closed: false,
    live: true,
    read_only: false,
    conversation: false,
    state: "working",
    step,
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
    rerender(screenOf("PROJ-1", [sess("a", false), sess("b", true, "implement")]));
    expect(selected()).toBe("Implementación");
  });

  it("does not steal the tab when the screen switches to another task", () => {
    const { rerender } = render(screenOf("PROJ-1", [sess("a", true)]));
    rerender(screenOf("PROJ-2", [sess("c", true)]));
    expect(selected()).toBe("Tarea");
  });
});

describe("TaskScreen base-stale banner", () => {
  afterEach(() => clearToasts());

  const stale: InboxItem = {
    id: "item-1", kind: "base-stale",
    task: "PROJ-1",
    title: "A task",
    session: null,
    text: "No se pudo actualizar r (sin red); se usó main",
    since: null,
    notice: 9,
  };
  const screenWith = (notices: InboxItem[], onChanged = () => {}) =>
    render(
      <>
        <TaskScreen owner="PROJ-1" task={task({ type: "running" })} sessions={[]} asks={[]} notices={notices} askPosition="right" onChanged={onChanged} />
        <ToastHost />
      </>,
    );

  it("shows the notice text and dismisses it", async () => {
    dismissNotice.mockClear();
    const onChanged = vi.fn();
    const { getByText } = screenWith([stale], onChanged);
    expect(getByText("No se pudo actualizar r (sin red); se usó main")).toBeTruthy();
    fireEvent.click(getByText("Descartar"));
    await waitFor(() => expect(onChanged).toHaveBeenCalled());
    expect(dismissNotice).toHaveBeenCalledWith(9);
  });

  it("says how far the base moved and merges it in, with the result as a toast", async () => {
    baseStatus.mockResolvedValueOnce({ base: "origin/main", behind_by: 3 });
    updateBranch.mockResolvedValueOnce({ base: "origin/main", merged: 3 });
    const onChanged = vi.fn();
    const { getByText, findByText } = screenWith([stale], onChanged);
    await findByText("main avanzó 3 commits");
    fireEvent.click(getByText("Actualizar rama"));
    await waitFor(() => expect(updateBranch).toHaveBeenCalledWith("PROJ-1"));
    await waitFor(() => expect(onChanged).toHaveBeenCalled());
    await waitFor(() => expect(toastsText()).toContain("Rama actualizada: 3 commits de main integrados"));
  });

  it("shows the daemon's refusal as an error toast", async () => {
    baseStatus.mockResolvedValueOnce({ base: "origin/main", behind_by: 1 });
    updateBranch.mockRejectedValueOnce(new ApiError("la rama tiene cambios sin guardar", 409, false, "workspace-dirty"));
    const { getByText, findByText } = screenWith([stale]);
    await findByText("main avanzó 1 commit");
    fireEvent.click(getByText("Actualizar rama"));
    await waitFor(() => expect(toastsText()).toContain("la rama tiene cambios sin guardar"));
  });

  it("offers no update when the base has not moved", async () => {
    baseStatus.mockResolvedValueOnce({ base: "origin/main", behind_by: 0 });
    const { queryByText } = screenWith([stale]);
    await waitFor(() => expect(baseStatus).toHaveBeenCalled());
    expect(queryByText("Actualizar rama")).toBeNull();
  });

  it("has no banner without notices", () => {
    const { container } = screenWith([]);
    expect(container.querySelector(".base-stale")).toBeNull();
  });
});

describe("TaskScreen done date", () => {
  it("the header of a done task shows the task's finished_at, with no events call", () => {
    const h = head({ ...task({ type: "done" }), finished_at: "2020-01-01T00:00:00Z" });
    expect(h.querySelector(".th-when")!.textContent).not.toBe("");
  });
});

describe("TaskScreen editable title", () => {
  const edit = (t: Task) => {
    const h = head(t);
    fireEvent.click(h.querySelector("h1")!);
    return screen.getByLabelText("Título de la tarea") as HTMLInputElement;
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
    expect(screen.queryByLabelText("Título de la tarea")).toBeNull();
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
  const stream: Session = { effective_runtime: "pty", runtime_override: null, last_activity_at: null, last_message: null, id: "s1", task: "PROJ-1", kind: "claude", label: "Main", primary: true, closed: false, live: true, read_only: false, step: null, state: "waiting", conversation: false, runtime: "stream" };
  const screen1 = (t: Task, s: Session) => (
    <TaskScreen owner="PROJ-1" task={t} sessions={[s]} asks={[]} notices={[]} askPosition="right" onChanged={() => {}} />
  );
  const chatTask = { ...task({ type: "running" }), chat_mode: true };

  beforeEach(() => {
    sessionRuntime.calls.mockClear();
    sessionRuntime.result = () => Promise.resolve(null);
    sessionFor("PROJ-1").active = sessionTab("s1");
  });

  it("has no Probar modo chat button: the switch lives in the header", () => {
    const r = render(screen1(task({ type: "running" }), { ...stream, runtime: "pty" }));
    expect(r.queryByRole("button", { name: /Probar modo chat/ })).toBeNull();
    expect(r.container.querySelector(".th .svs")).not.toBeNull();
    expect(r.container.querySelector(".etabs .svs")).toBeNull();
  });

  it("is left out of the Tarea tab", () => {
    sessionFor("PROJ-1").active = "\0overview";
    const r = render(screen1(chatTask, stream));
    expect(r.container.querySelector(".svs")).toBeNull();
  });

  it("Terminal on a chat-mode session confirms, changes the session's runtime and shows the terminal", async () => {
    const r = render(screen1(chatTask, stream));
    fireEvent.click(r.getByRole("button", { name: /^Terminal$/ }));
    expect(sessionRuntime.calls).not.toHaveBeenCalled();
    fireEvent.click(r.getByRole("button", { name: "Cambiar" }));
    expect(sessionRuntime.calls).toHaveBeenCalledWith("s1", "terminal");
    await waitFor(() => expect(r.queryByText("Pasar a terminal")).toBeNull());
    r.rerender(screen1({ ...chatTask, chat_mode: false }, { ...stream, runtime: "pty" }));
    // The terminal is what shows, not the chat the session no longer has.
    expect(r.getByRole("button", { name: /^Terminal$/ }).getAttribute("aria-pressed")).toBe("true");
    expect(r.getByRole("button", { name: /^Chat/ }).getAttribute("aria-pressed")).toBe("false");
  });

  it("Chat on a chat-mode session does not switch anything", () => {
    const r = render(screen1(chatTask, stream));
    fireEvent.click(r.getByRole("button", { name: /^Chat/ }));
    expect(sessionRuntime.calls).not.toHaveBeenCalled();
  });

  it("Chat on a terminal session confirms and switches the runtime to chat", () => {
    const r = render(screen1(task({ type: "running" }), { ...stream, runtime: "pty" }));
    fireEvent.click(r.getByRole("button", { name: /^Chat/ }));
    expect(sessionRuntime.calls).not.toHaveBeenCalled();
    fireEvent.click(r.getByRole("button", { name: "Cambiar" }));
    expect(sessionRuntime.calls).toHaveBeenCalledWith("s1", "chat");
  });

  it("shows a refused switch in Spanish, never the daemon's English", async () => {
    sessionRuntime.result = () => Promise.reject(new ApiError("wait for the turn to end or press Detener", 409));
    const r = render(screen1(chatTask, stream));
    fireEvent.click(r.getByRole("button", { name: /^Terminal$/ }));
    fireEvent.click(r.getByRole("button", { name: "Cambiar" }));
    const alert = await r.findByRole("alert");
    expect(alert.textContent).toContain("Espera a que termine el turno");
    expect(alert.textContent).not.toContain("wait for the turn");
  });

  it("clears the refusal bar after a later successful switch", async () => {
    sessionRuntime.result = () => Promise.reject(new ApiError("wait for the turn to end or press Detener", 409));
    const r = render(screen1(chatTask, stream));
    fireEvent.click(r.getByRole("button", { name: /^Terminal$/ }));
    fireEvent.click(r.getByRole("button", { name: "Cambiar" }));
    await r.findByRole("alert");
    sessionRuntime.result = () => Promise.resolve(null);
    fireEvent.click(r.getByRole("button", { name: /^Terminal$/ }));
    fireEvent.click(r.getByRole("button", { name: "Cambiar" }));
    await waitFor(() => expect(r.queryByRole("alert")).toBeNull());
  });

  it("clears the bar when the tab changes", async () => {
    sessionRuntime.result = () => Promise.reject(new ApiError("wait for the turn to end or press Detener", 409));
    const r = render(screen1(chatTask, stream));
    fireEvent.click(r.getByRole("button", { name: /^Terminal$/ }));
    fireEvent.click(r.getByRole("button", { name: "Cambiar" }));
    await r.findByRole("alert");
    fireEvent.click(r.getByRole("tab", { name: "Tarea" }));
    await waitFor(() => expect(r.queryByRole("alert")).toBeNull());
  });
});

describe("TaskScreen at 1280 (tree hidden below 1300)", () => {
  const narrow = (rail: boolean) =>
    vi.stubGlobal("matchMedia", (q: string) => ({
      matches: rail && q.includes("1299"),
      addEventListener: () => {},
      removeEventListener: () => {},
    }));
  const screenFor = () =>
    render(<TaskScreen owner="@web" project={project} sessions={[]} asks={[]} notices={[]} askPosition="right" onChanged={() => {}} />);
  afterEach(() => vi.unstubAllGlobals());

  it("hides the file tree below the rail breakpoint", () => {
    narrow(true);
    expect(screenFor().container.querySelector(".tscreen")!.className).not.toContain("with-tree");
  });

  it("lets the user open it with the toggle", () => {
    narrow(true);
    const r = screenFor();
    act(() => {
      window.dispatchEvent(new Event("delilah:toggle-tree"));
    });
    expect(r.container.querySelector(".tscreen")!.className).toContain("with-tree");
  });

  it("shows the tree on a wide window", () => {
    narrow(false);
    expect(screenFor().container.querySelector(".tscreen")!.className).toContain("with-tree");
  });
});

describe("TaskScreen Oficina tab", () => {
  it("is right after the task tab, not closable, and opens the floor", () => {
    head(task({ type: "running" }));
    const tabs = screen.getAllByRole("tab");
    expect(tabs[0].textContent).toBe("Tarea");
    expect(tabs[1].textContent).toBe("Oficina");
    expect(screen.queryByLabelText("Cerrar Oficina")).toBeNull();
    fireEvent.click(tabs[1]);
    expect(screen.getByTestId("office").dataset.owner).toBe("PROJ-1");
  });

  it("hands the app's connection state to the floor, so it opens no feed of its own", () => {
    const t = task({ type: "running" });
    sessionFor("PROJ-1").active = OFFICE;
    render(<TaskScreen owner="PROJ-1" task={t} online={false} sessions={[]} asks={[]} notices={[]} askPosition="right" onChanged={() => {}} />);
    expect(screen.getByTestId("office").dataset.online).toBe("false");
  });

  it("is also on a project screen and is restored when persisted as active", () => {
    sessionFor("@web").active = OFFICE;
    head();
    expect(screen.getAllByRole("tab")[1].textContent).toBe("Oficina");
    expect(screen.getByTestId("office")).not.toBeNull();
  });
});

describe("TaskScreen parent tasks", () => {
  const open = (t: Task) => {
    sessionFor(t.key).active = "\0overview";
    return render(<TaskScreen owner={t.key} task={t} sessions={[]} asks={[]} notices={[]} askPosition="right" onChanged={() => {}} />);
  };

  it("lists the subtasks of a parent task on the Tarea tab and shows the progress in the header", () => {
    open({ ...task({ type: "running" }), kind: "parent_task", subtask_progress: { merged: 0, counted: 0, dropped: 0, ready: false } });
    expect(document.querySelector(".subtasks")).not.toBeNull();
    expect(document.querySelector(".th-progress")).not.toBeNull();
  });

  it("shows no subtask list on a normal task", () => {
    open(task({ type: "running" }));
    expect(document.querySelector(".subtasks")).toBeNull();
  });
});
