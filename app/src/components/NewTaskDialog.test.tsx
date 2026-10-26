// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { api, ApiError, DEFAULT_PROJECT_SETTINGS, type BranchProposal, type Project, type Session, type Task } from "../api";
import { setLanguage } from "../i18n";
import { NewTaskDialog } from "./NewTaskDialog";

vi.mock("./WherePicker", async (orig) => ({ ...(await orig<typeof import("./WherePicker")>()), WherePicker: () => null }));

const web: Project = { name: "web", path: "/w/web", kind: "repo", workflow: "feature", settings: DEFAULT_PROJECT_SETTINGS };
/** A kit Toggle or Segmented option, found by its visible label. */
const btn = (root: HTMLElement, label: string) =>
  Array.from(root.querySelectorAll("button")).find((b) => b.textContent?.startsWith(label)) as HTMLButtonElement;
const pressed = (b: HTMLButtonElement) => b.getAttribute("aria-pressed") === "true";
const session = { id: "s9", task: "@web", kind: "claude", conversation: true } as Session;

function setup() {
  vi.spyOn(api, "preference").mockResolvedValue(null);
  const props = { onClose: vi.fn(), onStarted: vi.fn(), onSaved: vi.fn(), onConversation: vi.fn() };
  const view = render(<NewTaskDialog projects={[web]} {...props} />);
  return { ...view, ...props };
}

beforeEach(() => {
  setLanguage("es");
  vi.spyOn(api, "submodules").mockResolvedValue([]);
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  localStorage.clear();
});

describe("NewTaskDialog origins", () => {
  it("Con Claude opens a conversation with the idea and hides the task fields", async () => {
    const open = vi.spyOn(api, "openProjectSession").mockResolvedValue(session);
    const { getByText, getByLabelText, queryByPlaceholderText, container, onConversation } = setup();
    fireEvent.click(getByText("Con Claude"));
    expect(queryByPlaceholderText("PROJ-1234")).toBeNull();
    fireEvent.change(getByLabelText("Tu idea"), { target: { value: "cache rates" } });
    fireEvent.submit(container.querySelector("form")!);
    await waitFor(() => expect(onConversation).toHaveBeenCalledWith("web", session));
    expect(open).toHaveBeenCalledWith("web", { kind: "claude", conversation: true, prompt: "cache rates" });
  });

  it("Con Claude without an idea sends no prompt", async () => {
    const open = vi.spyOn(api, "openProjectSession").mockResolvedValue(session);
    const { getByText, container, onConversation } = setup();
    fireEvent.click(getByText("Con Claude"));
    fireEvent.submit(container.querySelector("form")!);
    await waitFor(() => expect(onConversation).toHaveBeenCalled());
    expect(open).toHaveBeenCalledWith("web", { kind: "claude", conversation: true, prompt: undefined });
  });

  it("the Jira path still runs a task", async () => {
    const task = { key: "PROJ-1" } as never;
    const run = vi.spyOn(api, "run").mockResolvedValue(task);
    const { getByPlaceholderText, container, onStarted } = setup();
    fireEvent.change(getByPlaceholderText("PROJ-1234"), { target: { value: "PROJ-1" } });
    fireEvent.change(container.querySelector("input:not(.mono)")!, { target: { value: "Fix" } });
    fireEvent.submit(container.querySelector("form")!);
    await waitFor(() => expect(onStarted).toHaveBeenCalledWith(task));
    expect(run.mock.calls[0][0]).toMatchObject({ key: "PROJ-1", title: "Fix", free: false });
  });

  it("the Libre path still runs a free task", async () => {
    const task = { key: "LIBRE-1" } as never;
    const run = vi.spyOn(api, "run").mockResolvedValue(task);
    const { getByText, getByPlaceholderText, container, onStarted } = setup();
    fireEvent.click(getByText("Tarea libre"));
    fireEvent.change(getByPlaceholderText(/caché de tarifas/), { target: { value: "Do it" } });
    fireEvent.submit(container.querySelector("form")!);
    await waitFor(() => expect(onStarted).toHaveBeenCalledWith(task));
    expect(run.mock.calls[0][0]).toMatchObject({ key: "", title: "Do it", free: true });
  });
});

const phasedSettings = { ...DEFAULT_PROJECT_SETTINGS, workflow: { kind: "phased" as const, plan: "/p", implement: "/i", review: "/r" } };
const projects: Project[] = [{ name: "web", path: "/p/web", kind: "repo", workflow: "feature", settings: phasedSettings }];
const started: Task = {
  created_at: "2026-01-01T00:00:00Z", finished_at: null, key: "LIBRE-1",
  title: "t",
  project: "web",
  workflow: "feature",
  phase: null,
  status: { type: "queued" },
  saving_mode: true, title_source: "initial", previous_title: null, branch_error: null,
  autopilot: null,
  handoff: null,
};

describe("NewTaskDialog saving mode", () => {
  it("is preloaded from the saving-mode preference and sent with the run", async () => {
    vi.spyOn(api, "preference").mockResolvedValue(true);
    const run = vi.spyOn(api, "run").mockResolvedValueOnce(started);
    const { container } = render(
      <NewTaskDialog projects={projects} draft={{ key: "PROJ-1", title: "Algo", components: [] }} onClose={() => {}} onStarted={() => {}} onSaved={() => {}} onConversation={() => {}} />,
    );
    await waitFor(() => expect(pressed(btn(container, "Ahorro"))).toBe(true));
    fireEvent.submit(container.querySelector("form")!);
    await waitFor(() => expect(run).toHaveBeenCalled());
    expect(run.mock.calls[0][0].saving_mode).toBe(true);
  });

  it("defaults to off and can be switched on, also for the backlog", async () => {
    vi.spyOn(api, "preference").mockResolvedValue(null);
    const create = vi.spyOn(api, "createTask").mockResolvedValueOnce(started);
    const { container, getByText } = render(
      <NewTaskDialog projects={projects} draft={{ key: "PROJ-1", title: "Algo", components: [] }} onClose={() => {}} onStarted={() => {}} onSaved={() => {}} onConversation={() => {}} />,
    );
    const box = btn(container, "Ahorro");
    expect(pressed(box)).toBe(false);
    fireEvent.click(box);
    fireEvent.click(getByText("Guardar en backlog"));
    await waitFor(() => expect(create).toHaveBeenCalled());
    expect(create.mock.calls[0][0].saving_mode).toBe(true);
  });

  it("starts a backlog task with the mode it was saved with", async () => {
    const pref = vi.spyOn(api, "preference").mockResolvedValue(false);
    const { container } = render(
      <NewTaskDialog projects={projects} existing={{ ...started, saving_mode: true }} onClose={() => {}} onStarted={() => {}} onSaved={() => {}} onConversation={() => {}} />,
    );
    await Promise.resolve();
    expect(pressed(btn(container, "Ahorro"))).toBe(true);
    expect(pref).not.toHaveBeenCalled();
  });
});

describe("NewTaskDialog autopilot", () => {
  const draft = { key: "PROJ-1", title: "Algo", components: [] };
  const mount = () =>
    render(<NewTaskDialog projects={projects} draft={draft} onClose={() => {}} onStarted={() => {}} onSaved={() => {}} onConversation={() => {}} />);

  it("turning it on asks first, then forces Ahorro on and locked; turning it off unlocks it, still on", async () => {
    vi.spyOn(api, "preference").mockResolvedValue(null);
    const { container, getByText } = mount();
    const auto = btn(container, "Autopiloto");
    const saving = btn(container, "Ahorro");
    fireEvent.click(auto);
    expect(pressed(auto)).toBe(false);
    fireEvent.click(getByText("Activar"));
    expect(pressed(auto)).toBe(true);
    expect(pressed(saving)).toBe(true);
    expect(saving.disabled).toBe(true);
    fireEvent.click(auto);
    expect(saving.disabled).toBe(false);
    expect(pressed(saving)).toBe(true);
  });

  it("is offered for a project whose workflow is \"project\"", () => {
    const { container } = render(
      <NewTaskDialog projects={[{ ...projects[0], workflow: "project" }]} draft={draft} onClose={() => {}} onStarted={() => {}} onSaved={() => {}} onConversation={() => {}} />,
    );
    expect(btn(container, "Autopiloto")).toBeDefined();
  });

  it("is sent with the run together with saving mode", async () => {
    vi.spyOn(api, "preference").mockResolvedValue(null);
    const run = vi.spyOn(api, "run").mockResolvedValueOnce(started);
    const { container, getByText } = mount();
    fireEvent.click(btn(container, "Autopiloto"));
    fireEvent.click(getByText("Activar"));
    fireEvent.submit(container.querySelector("form")!);
    await waitFor(() => expect(run).toHaveBeenCalled());
    expect(run.mock.calls[0][0].autopilot).toBe(true);
    expect(run.mock.calls[0][0].saving_mode).toBe(true);
  });

  it("is sent when saving to the backlog", async () => {
    vi.spyOn(api, "preference").mockResolvedValue(null);
    const create = vi.spyOn(api, "createTask").mockResolvedValueOnce(started);
    const { container, getByText } = mount();
    fireEvent.click(btn(container, "Autopiloto"));
    fireEvent.click(getByText("Activar"));
    fireEvent.click(getByText("Guardar en backlog"));
    await waitFor(() => expect(create).toHaveBeenCalled());
    expect(create.mock.calls[0][0]).toMatchObject({ autopilot: true, saving_mode: true });
  });

  it("says it needs the project's workflow when the task uses a custom prompt", () => {
    vi.spyOn(api, "preference").mockResolvedValue(null);
    const existing = { ...started, workflow: "prompt" };
    const { container } = render(
      <NewTaskDialog projects={projects} existing={existing} onClose={() => {}} onStarted={() => {}} onSaved={() => {}} onConversation={() => {}} />,
    );
    expect(btn(container, "Autopiloto")).toBeUndefined();
    expect(container.textContent).toContain("no con un prompt propio");
    expect(container.textContent).not.toContain("flujo por fases");
  });

  it("unticking it on a backlog task that has it sends autopilot: false", async () => {
    vi.spyOn(api, "preference").mockResolvedValue(null);
    const run = vi.spyOn(api, "run").mockResolvedValueOnce(started);
    const existing = { ...started, autopilot: { state: "running" as const, since: "2026-10-21T10:00:00Z" } };
    const { container } = render(
      <NewTaskDialog projects={projects} existing={existing} onClose={() => {}} onStarted={() => {}} onSaved={() => {}} onConversation={() => {}} />,
    );
    fireEvent.click(btn(container, "Autopiloto"));
    fireEvent.submit(container.querySelector("form")!);
    await waitFor(() => expect(run).toHaveBeenCalled());
    expect(run.mock.calls[0][0].autopilot).toBe(false);
  });

  it("is hidden, with a reason, when the project's workflow is not phased", () => {
    vi.spyOn(api, "preference").mockResolvedValue(null);
    const { container } = render(
      <NewTaskDialog projects={[{ ...web, workflow: "custom" }]} draft={draft} onClose={() => {}} onStarted={() => {}} onSaved={() => {}} onConversation={() => {}} />,
    );
    expect(btn(container, "Autopiloto")).toBeUndefined();
    expect(btn(container, "Ahorro")).toBeUndefined();
    expect(container.textContent).toContain("solo están disponibles con un flujo por fases");
  });

  it("a non-phased task is sent with saving mode off even if the preference is on", async () => {
    vi.spyOn(api, "preference").mockResolvedValue(true);
    const run = vi.spyOn(api, "run").mockResolvedValueOnce(started);
    const { container } = render(
      <NewTaskDialog projects={[web]} draft={draft} onClose={() => {}} onStarted={() => {}} onSaved={() => {}} onConversation={() => {}} />,
    );
    await Promise.resolve();
    fireEvent.submit(container.querySelector("form")!);
    await waitFor(() => expect(run).toHaveBeenCalled());
    expect(run.mock.calls[0][0].saving_mode).toBe(false);
  });
});

describe("NewTaskDialog view", () => {
  const draft = { key: "PROJ-1", title: "Algo", components: [] };
  const mount = (extra: Partial<React.ComponentProps<typeof NewTaskDialog>> = {}) =>
    render(<NewTaskDialog projects={projects} draft={draft} onClose={() => {}} onStarted={() => {}} onSaved={() => {}} onConversation={() => {}} {...extra} />);

  it("defaults to Chat and is sent with the run", async () => {
    vi.spyOn(api, "preference").mockResolvedValue(null);
    const run = vi.spyOn(api, "run").mockResolvedValueOnce(started);
    const { container } = mount();
    expect(btn(container, "Chat").getAttribute("aria-checked")).toBe("true");
    expect(btn(container, "Chat").textContent).toContain("beta");
    fireEvent.click(btn(container, "Terminal"));
    fireEvent.submit(container.querySelector("form")!);
    await waitFor(() => expect(run).toHaveBeenCalled());
    expect(run.mock.calls[0][0]).toMatchObject({ view: "terminal" });
  });

  it("is sent when saving to the backlog", async () => {
    vi.spyOn(api, "preference").mockResolvedValue(null);
    const create = vi.spyOn(api, "createTask").mockResolvedValueOnce(started);
    const { getByText } = mount();
    fireEvent.click(getByText("Guardar en backlog"));
    await waitFor(() => expect(create).toHaveBeenCalled());
    expect(create.mock.calls[0][0]).toMatchObject({ view: "chat" });
  });

  it("starts from the view of a backlog task", () => {
    vi.spyOn(api, "preference").mockResolvedValue(null);
    const { container } = mount({ draft: undefined, existing: { ...started, view: "terminal" } });
    expect(btn(container, "Terminal").getAttribute("aria-checked")).toBe("true");
  });
});

const branchProjects: Project[] = [{ name: "app", kind: "repo", settings: DEFAULT_PROJECT_SETTINGS } as Project];

const proposal = (branch: string, source: BranchProposal["source"] = "claude"): BranchProposal => ({ branch, source });

function setupBranch(props: Partial<React.ComponentProps<typeof NewTaskDialog>> = {}, free = true) {
  const utils = render(<NewTaskDialog projects={branchProjects} onClose={() => {}} onStarted={() => {}} onSaved={() => {}} onConversation={() => {}} {...props} />);
  const radios = utils.container.querySelectorAll("[role=radio]");
  if (free && radios[1]) fireEvent.click(radios[1]); // free task: no ticket key needed
  const field = () => utils.getByLabelText("Branch") as HTMLInputElement;
  const title = () => Array.from(utils.container.querySelectorAll("input[required]")).pop() as HTMLInputElement;
  return { ...utils, field, title };
}

const type = (el: HTMLInputElement, value: string) => fireEvent.change(el, { target: { value } });
const wait = (ms: number) => act(async () => { await vi.advanceTimersByTimeAsync(ms); });

describe("NewTaskDialog branch field", () => {
  beforeEach(() => {
    setLanguage("en");
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("proposes after 1 s and shows the source", async () => {
    const spy = vi.spyOn(api, "proposeBranch").mockResolvedValue(proposal("feature/arreglo-del-login"));
    const { field, title, container } = setupBranch({}, false);
    type(title(), "Arreglo del login");
    await wait(900);
    expect(spy).not.toHaveBeenCalled();
    await wait(200);
    expect(spy).toHaveBeenCalledTimes(1);
    expect(field().value).toBe("arreglo-del-login");
    expect(container.textContent).toContain("feature/");
    expect(container.textContent).toContain("proposed by Claude");
  });

  it("does not overwrite what the developer edited; clearing re-enables proposals", async () => {
    const spy = vi.spyOn(api, "proposeBranch").mockResolvedValue(proposal("feature/one", "title"));
    const { field, title } = setupBranch({}, false);
    type(title(), "One");
    await wait(1100);
    expect(field().value).toBe("one");
    type(field(), "mine");
    spy.mockResolvedValue(proposal("feature/two", "title"));
    type(title(), "Two");
    await wait(1100);
    expect(field().value).toBe("mine");
    type(field(), "");
    type(title(), "Three");
    await wait(1100);
    expect(field().value).toBe("two");
  });

  it("sends branch when non-empty and omits it when empty", async () => {
    vi.spyOn(api, "proposeBranch").mockResolvedValue({ branch: null, source: "title" });
    const run = vi.spyOn(api, "run").mockRejectedValue(new Error("x"));
    const { field, title, getByText } = setupBranch();
    type(title(), "Algo");
    await wait(1100);
    fireEvent.click(getByText("Start", { selector: "button" }));
    await wait(0);
    expect(run.mock.calls[0][0]).not.toHaveProperty("branch");
    type(field(), "feature/arreglo-del-login");
    expect(field().value).toBe("arreglo-del-login");
    fireEvent.click(getByText("Start", { selector: "button" }));
    await wait(0);
    expect(run.mock.calls[1][0].branch).toBe("feature/arreglo-del-login");
  });

  it("ignores a stale proposal", async () => {
    const resolvers: ((p: BranchProposal) => void)[] = [];
    vi.spyOn(api, "proposeBranch").mockImplementation(() => new Promise((r) => resolvers.push(r)));
    const { field, title } = setupBranch({}, false);
    type(title(), "a");
    await wait(1100);
    type(title(), "ab");
    await wait(1100);
    expect(resolvers).toHaveLength(2);
    await act(async () => resolvers[1](proposal("feature/new")));
    await act(async () => resolvers[0](proposal("feature/old")));
    expect(field().value).toBe("new");
  });

  it("leaves the field empty when the proposal fails", async () => {
    vi.spyOn(api, "proposeBranch").mockRejectedValue(new Error("boom"));
    const { field, title, container } = setupBranch({}, false);
    type(title(), "a");
    await wait(1100);
    expect(field().value).toBe("");
    expect(field().placeholder).toBe("automatic if left empty");
    expect(container.querySelector(".error")).toBeNull();
  });

  it("keeps spaces typed character by character and submits them", async () => {
    vi.spyOn(api, "proposeBranch").mockResolvedValue({ branch: null, source: "title" });
    const run = vi.spyOn(api, "run").mockRejectedValue(new Error("x"));
    const { field, title, getByText } = setupBranch();
    type(title(), "Algo");
    let acc = "";
    for (const ch of "Arreglo del login") {
      acc += ch;
      type(field(), acc);
    }
    expect(field().value).toBe("Arreglo del login");
    fireEvent.click(getByText("Start", { selector: "button" }));
    await wait(0);
    expect(run.mock.calls[0][0].branch).toBe("feature/Arreglo del login");
  });

  it("proposes again when the notes change", async () => {
    const spy = vi.spyOn(api, "proposeBranch").mockResolvedValue(proposal("feature/one", "title"));
    const { title, container } = setupBranch({}, false);
    type(title(), "One");
    await wait(1100);
    expect(spy).toHaveBeenCalledTimes(1);
    const notes = container.querySelector("textarea") as HTMLTextAreaElement;
    fireEvent.change(notes, { target: { value: "more context" } });
    await wait(900);
    expect(spy).toHaveBeenCalledTimes(1);
    await wait(200);
    expect(spy).toHaveBeenCalledTimes(2);
    expect(spy.mock.calls[1][1]).toMatchObject({ notes: "more context" });
  });

  it("submits the shown proposal for a Jira draft", async () => {
    vi.spyOn(api, "proposeBranch").mockResolvedValue(proposal("feature/PROJ-12-retry-failed-payments"));
    const run = vi.spyOn(api, "run").mockRejectedValue(new Error("x"));
    const { getByText } = setupBranch({ draft: { key: "PROJ-12", title: "Retry failed payments", components: [] } });
    await wait(1100);
    fireEvent.click(getByText("Start", { selector: "button" }));
    await wait(0);
    expect(run.mock.calls[0][0].branch).toBe("feature/PROJ-12-retry-failed-payments");
  });

  it("does not propose nor send a branch for a free task without a template; says the analysis creates it", async () => {
    const spy = vi.spyOn(api, "proposeBranch").mockResolvedValue(proposal("feature/x"));
    const run = vi.spyOn(api, "run").mockRejectedValue(new Error("x"));
    const { field, title, getByText } = setupBranch();
    type(title(), "Arreglo del login");
    await wait(1500);
    expect(spy).not.toHaveBeenCalled();
    expect(field().value).toBe("");
    expect(field().placeholder).toBe("the analysis will create it");
    fireEvent.click(getByText("Start", { selector: "button" }));
    await wait(0);
    expect(run.mock.calls[0][0]).not.toHaveProperty("branch");
    expect(run.mock.calls[0][0].free).toBe(true);
  });

  it("sends the branch a free task's user typed", async () => {
    const spy = vi.spyOn(api, "proposeBranch").mockResolvedValue(proposal("feature/x"));
    const run = vi.spyOn(api, "run").mockRejectedValue(new Error("x"));
    const { field, title, getByText } = setupBranch();
    type(title(), "Algo");
    type(field(), "mine");
    await wait(1500);
    expect(spy).not.toHaveBeenCalled();
    fireEvent.click(getByText("Start", { selector: "button" }));
    await wait(0);
    expect(run.mock.calls[0][0].branch).toBe("feature/mine");
  });

  it("proposes and sends no branch for a backlog LIBRE task in a project without a template", async () => {
    const spy = vi.spyOn(api, "proposeBranch").mockResolvedValue(proposal("feature/x"));
    const run = vi.spyOn(api, "run").mockRejectedValue(new Error("x"));
    const existing = { key: "LIBRE-3", title: "Arreglo", project: "app", in_checkout: true } as Task;
    const { field, getByText } = setupBranch({ existing }, false);
    await wait(1500);
    expect(spy).not.toHaveBeenCalled();
    expect(field().value).toBe("");
    fireEvent.click(getByText("Start", { selector: "button" }));
    await wait(0);
    expect(run.mock.calls[0][0]).not.toHaveProperty("branch");
  });

  it("keeps proposing for a free task when the project has a template", async () => {
    const spy = vi.spyOn(api, "proposeBranch").mockResolvedValue(proposal("fix/libre-1-algo"));
    const projects = [{ name: "app", kind: "repo", settings: { ...DEFAULT_PROJECT_SETTINGS, branch: "fix/{ticket}-{slug}" } } as Project];
    const { title } = setupBranch({ projects });
    type(title(), "Algo");
    await wait(1100);
    expect(spy).toHaveBeenCalledTimes(1);
  });

  it("shows the field for existing and fromSession", () => {
    vi.spyOn(api, "proposeBranch").mockResolvedValue(proposal("feature/x"));
    const existing = { key: "LIBRE-3", title: "T", project: "app", in_checkout: true } as Task;
    expect(setupBranch({ existing }).field()).toBeTruthy();
    cleanup();
    expect(setupBranch({ fromSession: { id: "s", label: "L" }, project: "app" }).field()).toBeTruthy();
  });
});

describe("NewTaskDialog chat mode", () => {
  const draft = { key: "PROJ-1", title: "Algo", components: [] };
  const mount = (extra: Partial<React.ComponentProps<typeof NewTaskDialog>> = {}) =>
    render(<NewTaskDialog projects={projects} draft={draft} onClose={() => {}} onStarted={() => {}} onSaved={() => {}} onConversation={() => {}} {...extra} />);

  it("has no checkbox any more and sends no chat_mode by default", async () => {
    vi.spyOn(api, "preference").mockResolvedValue(null);
    const run = vi.spyOn(api, "run").mockResolvedValueOnce(started);
    const { container } = mount();
    expect(container.querySelector('input[type="checkbox"]')).toBeNull();
    fireEvent.submit(container.querySelector("form")!);
    await waitFor(() => expect(run).toHaveBeenCalled());
    expect(run.mock.calls[0][0].chat_mode).toBeUndefined();
  });

  it("a backlog task saved with chat mode keeps sending it", async () => {
    vi.spyOn(api, "preference").mockResolvedValue(null);
    const run = vi.spyOn(api, "run").mockResolvedValueOnce(started);
    const { container } = mount({ draft: undefined, existing: { ...started, chat_mode: true } });
    fireEvent.submit(container.querySelector("form")!);
    await waitFor(() => expect(run).toHaveBeenCalled());
    expect(run.mock.calls[0][0].chat_mode).toBe(true);
  });

  it("offers to start in the terminal when chat mode could not start", async () => {
    vi.spyOn(api, "preference").mockResolvedValue(null);
    const run = vi
      .spyOn(api, "run")
      .mockRejectedValueOnce(new Error("chat mode could not start: no claude"))
      .mockResolvedValueOnce(started);
    const { getByText, container } = mount({ draft: undefined, existing: { ...started, chat_mode: true } });
    fireEvent.submit(container.querySelector("form")!);
    await waitFor(() => getByText("Iniciar en terminal"));
    fireEvent.click(getByText("Iniciar en terminal"));
    await waitFor(() => expect(run).toHaveBeenCalledTimes(2));
    expect(run.mock.calls[1][0].chat_mode).toBeFalsy();
  });
});

describe("NewTaskDialog layout and errors", () => {
  const draft = { key: "PROJ-1", title: "Algo", components: [] };
  const mount = (extra: Partial<React.ComponentProps<typeof NewTaskDialog>> = {}) =>
    render(<NewTaskDialog projects={projects} draft={draft} onClose={() => {}} onStarted={() => {}} onSaved={() => {}} onConversation={() => {}} {...extra} />);

  it("is a 600x700 dialog with the actions in a footer outside the scrolling body, «Iniciar» primary", () => {
    vi.spyOn(api, "preference").mockResolvedValue(null);
    const { container, getByText } = mount();
    const dialog = container.querySelector('[role="dialog"]') as HTMLElement;
    expect(dialog.style.width).toBe("600px");
    expect(dialog.style.height).toBe("700px");
    const footer = dialog.querySelector(".dialog-ft")!;
    const names = Array.from(footer.querySelectorAll("button")).map((b) => b.textContent);
    expect(names).toEqual(["Cancelar", "Guardar en backlog", "Iniciar"]);
    expect(footer.textContent).toContain("⌘↵ Iniciar");
    expect(dialog.querySelector(".dialog-bd")!.contains(footer)).toBe(false);
    expect(getByText("Iniciar").className).toContain("b-p");
    expect(container.textContent).not.toContain("Lanzar");
  });

  it("puts Vista and Rama under Opciones avanzadas", () => {
    vi.spyOn(api, "preference").mockResolvedValue(null);
    const { container } = mount();
    const adv = container.querySelector("details")!;
    expect(adv.querySelector("summary")!.textContent).toBe("Opciones avanzadas");
    expect(adv.querySelector('input[aria-label="Rama"]')).not.toBeNull();
    expect(adv.querySelector('[role="radiogroup"]')).not.toBeNull();
  });

  it("explains Ahorro in the new words", () => {
    vi.spyOn(api, "preference").mockResolvedValue(null);
    const { getByText } = mount();
    expect(getByText("Ahorro: una sesión de Claude nueva por fase, para gastar menos contexto")).toBeTruthy();
  });

  it("shows inline field errors, and does not run, when the ticket and title are missing", () => {
    vi.spyOn(api, "preference").mockResolvedValue(null);
    const run = vi.spyOn(api, "run");
    const { container, getByText } = mount({ draft: undefined });
    fireEvent.click(getByText("Iniciar"));
    expect(run).not.toHaveBeenCalled();
    const alerts = Array.from(container.querySelectorAll(".ntd-err")).map((e) => e.textContent);
    expect(alerts).toEqual(["Falta el ticket", "Falta el título"]);
  });

  it("Cmd+Enter starts the task", async () => {
    vi.spyOn(api, "preference").mockResolvedValue(null);
    const run = vi.spyOn(api, "run").mockResolvedValueOnce(started);
    const { container } = mount();
    fireEvent.keyDown(container.querySelector("form")!, { key: "Enter", metaKey: true });
    await waitFor(() => expect(run).toHaveBeenCalled());
  });

  it("a dirty checkout refusal reads as one human error and offers «Iniciar en un worktree», which retries outside the checkout", async () => {
    vi.spyOn(api, "preference").mockResolvedValue(null);
    const run = vi
      .spyOn(api, "run")
      .mockRejectedValueOnce(new ApiError("workspace: the checkout holds work", 409, false, "checkout-dirty"))
      .mockResolvedValueOnce(started);
    const { container, getByText, queryByText } = mount();
    fireEvent.submit(container.querySelector("form")!);
    await waitFor(() => getByText(/Tu checkout tiene cambios sin guardar/));
    expect(queryByText(/uncommitted/)).toBeNull();
    expect(run.mock.calls[0][0].in_checkout).toBe(true);
    fireEvent.click(getByText("Iniciar en un worktree"));
    await waitFor(() => expect(run).toHaveBeenCalledTimes(2));
    expect(run.mock.calls[1][0].in_checkout).toBe(false);
  });

  it("a dirty checkout from an older daemon (no reason) is still recognised by its message", async () => {
    vi.spyOn(api, "preference").mockResolvedValue(null);
    vi.spyOn(api, "run").mockRejectedValueOnce(new Error("/p/web has uncommitted changes; the checkout must be clean"));
    const { container, getByText } = mount();
    fireEvent.submit(container.querySelector("form")!);
    await waitFor(() => getByText(/Tu checkout tiene cambios sin guardar/));
  });

  it("another 409 reason is not shown as a dirty checkout even if its message says so", async () => {
    vi.spyOn(api, "preference").mockResolvedValue(null);
    vi.spyOn(api, "run").mockRejectedValueOnce(new ApiError("has uncommitted changes", 409, false, "other"));
    const { container, queryByText, findByText } = mount();
    fireEvent.submit(container.querySelector("form")!);
    await findByText(/has uncommitted changes/);
    expect(queryByText(/Tu checkout tiene cambios sin guardar/)).toBeNull();
  });

  it("«Abrir una terminal» opens a shell session in the project", async () => {
    vi.spyOn(api, "preference").mockResolvedValue(null);
    vi.spyOn(api, "run").mockRejectedValueOnce(new Error("checkout has uncommitted changes"));
    const open = vi.spyOn(api, "openProjectSession").mockResolvedValue(session);
    const onConversation = vi.fn();
    const { container, getByText } = mount({ onConversation });
    fireEvent.submit(container.querySelector("form")!);
    await waitFor(() => getByText("Abrir una terminal"));
    fireEvent.click(getByText("Abrir una terminal"));
    await waitFor(() => expect(onConversation).toHaveBeenCalledWith("web", session));
    expect(open).toHaveBeenCalledWith("web", { kind: "shell" });
  });

  it("any other error is shown as the daemon says it", async () => {
    vi.spyOn(api, "preference").mockResolvedValue(null);
    vi.spyOn(api, "run").mockRejectedValueOnce(new Error("boom"));
    const { container, getByText, queryByText } = mount();
    fireEvent.submit(container.querySelector("form")!);
    await waitFor(() => getByText("boom"));
    expect(queryByText("Iniciar en un worktree")).toBeNull();
  });
});

describe("NewTaskDialog start override", () => {
  const draft = { key: "PROJ-1", title: "Algo", components: [] };
  const withWorkflow = (workflow: Project["settings"]["workflow"]): Project[] => [{ ...projects[0], workflow: "project", settings: { ...DEFAULT_PROJECT_SETTINGS, workflow } }];
  const mountWith = (ps: Project[]) =>
    render(<NewTaskDialog projects={ps} draft={draft} onClose={() => {}} onStarted={() => {}} onSaved={() => {}} onConversation={() => {}} />);
  const startSelect = (c: HTMLElement) => c.querySelector('select[aria-label="Al empezar"]') as HTMLSelectElement;

  beforeEach(() => {
    vi.spyOn(api, "preference").mockResolvedValue(null);
    vi.spyOn(api, "projectSkills").mockResolvedValue([]);
  });

  it("shows what the project's workflow resolves to, filled with the typed ticket and title", () => {
    const { container } = mountWith(withWorkflow({ kind: "single", start: { type: "message", template: "/fix {ticket} {title}" } }));
    expect(startSelect(container).value).toBe("project");
    expect(container.querySelector(".start-resolved")!.textContent).toBe("/fix PROJ-1 Algo");
  });

  it("shows the plan of a phased project", () => {
    const { container } = mountWith(withWorkflow({ kind: "phased", plan: "/feature {task}", implement: "/i", review: "/r" }));
    expect(container.querySelector(".start-resolved")!.textContent).toBe("/feature PROJ-1");
  });

  it("sends no override for the project's own start", async () => {
    const run = vi.spyOn(api, "run").mockResolvedValueOnce(started);
    const { container } = mountWith(projects);
    fireEvent.submit(container.querySelector("form")!);
    await waitFor(() => expect(run).toHaveBeenCalled());
    expect(run.mock.calls[0][0].workflow).toBeUndefined();
    expect(run.mock.calls[0][0].prompt).toBeUndefined();
  });

  it("another message is sent as workflow prompt, and hides saving mode and autopilot", async () => {
    const run = vi.spyOn(api, "run").mockResolvedValueOnce(started);
    const { container } = mountWith(projects);
    expect(btn(container, "Autopiloto")).toBeDefined();
    fireEvent.change(startSelect(container), { target: { value: "prompt" } });
    fireEvent.change(container.querySelector('input[aria-label="Comando"]')!, { target: { value: "/fix {ticket}" } });
    expect(btn(container, "Autopiloto")).toBeUndefined();
    expect(btn(container, "Ahorro")).toBeUndefined();
    expect(container.textContent).toContain("no con un prompt propio");
    fireEvent.submit(container.querySelector("form")!);
    await waitFor(() => expect(run).toHaveBeenCalled());
    expect(run.mock.calls[0][0]).toMatchObject({ workflow: "prompt", prompt: "/fix {ticket}", saving_mode: false });
  });

  it("Nada is sent as workflow none and hides the phased controls; it is also saved to the backlog", async () => {
    const create = vi.spyOn(api, "createTask").mockResolvedValueOnce(started);
    const { container, getByText } = mountWith(projects);
    fireEvent.change(startSelect(container), { target: { value: "none" } });
    expect(btn(container, "Autopiloto")).toBeUndefined();
    fireEvent.click(getByText("Guardar en backlog"));
    await waitFor(() => expect(create).toHaveBeenCalled());
    expect(create.mock.calls[0][0]).toMatchObject({ workflow: "none" });
  });

  it("shows the daemon's template errors inline", async () => {
    vi.spyOn(api, "run").mockRejectedValueOnce(new ApiError("invalid", 422, false, undefined, undefined, [{ field: "workflow.start.template", code: "x", message: "placeholder desconocido" }]));
    const { container } = mountWith(projects);
    fireEvent.change(startSelect(container), { target: { value: "prompt" } });
    fireEvent.change(container.querySelector('input[aria-label="Comando"]')!, { target: { value: "/x {nope}" } });
    fireEvent.submit(container.querySelector("form")!);
    await waitFor(() => expect(container.querySelector('[data-field="workflow.start.template"]')).not.toBeNull());
  });

  it("a backlog task keeps its stored message", () => {
    const { container } = render(
      <NewTaskDialog projects={projects} existing={{ ...started, workflow: "prompt", start_message: "/mine" }} onClose={() => {}} onStarted={() => {}} onSaved={() => {}} onConversation={() => {}} />,
    );
    expect(startSelect(container).value).toBe("prompt");
    expect((container.querySelector('input[aria-label="Comando"]') as HTMLInputElement).value).toBe("/mine");
  });
});
