// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { api, DEFAULT_PROJECT_SETTINGS, type BranchProposal, type Project, type Session, type Task } from "../api";
import { setLanguage } from "../i18n";
import { NewTaskDialog } from "./NewTaskDialog";

vi.mock("./WherePicker", async (orig) => ({ ...(await orig<typeof import("./WherePicker")>()), WherePicker: () => null }));

const web: Project = { name: "web", path: "/w/web", kind: "repo", workflow: "feature", settings: DEFAULT_PROJECT_SETTINGS };
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

const projects: Project[] = [{ name: "web", path: "/p/web", kind: "repo", workflow: "feature", settings: DEFAULT_PROJECT_SETTINGS }];
const started: Task = {
  key: "LIBRE-1",
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
    const { getByLabelText, container } = render(
      <NewTaskDialog projects={projects} draft={{ key: "PROJ-1", title: "Algo", components: [] }} onClose={() => {}} onStarted={() => {}} onSaved={() => {}} onConversation={() => {}} />,
    );
    await waitFor(() => expect((getByLabelText("Modo ahorro") as HTMLInputElement).checked).toBe(true));
    fireEvent.submit(container.querySelector("form")!);
    await waitFor(() => expect(run).toHaveBeenCalled());
    expect(run.mock.calls[0][0].saving_mode).toBe(true);
  });

  it("defaults to off and can be switched on, also for the backlog", async () => {
    vi.spyOn(api, "preference").mockResolvedValue(null);
    const create = vi.spyOn(api, "createTask").mockResolvedValueOnce(started);
    const { getByLabelText, getByText } = render(
      <NewTaskDialog projects={projects} draft={{ key: "PROJ-1", title: "Algo", components: [] }} onClose={() => {}} onStarted={() => {}} onSaved={() => {}} onConversation={() => {}} />,
    );
    const box = getByLabelText("Modo ahorro") as HTMLInputElement;
    expect(box.checked).toBe(false);
    fireEvent.click(box);
    fireEvent.click(getByText("Guardar en backlog"));
    await waitFor(() => expect(create).toHaveBeenCalled());
    expect(create.mock.calls[0][0].saving_mode).toBe(true);
  });

  it("starts a backlog task with the mode it was saved with", async () => {
    const pref = vi.spyOn(api, "preference").mockResolvedValue(false);
    const { getByLabelText } = render(
      <NewTaskDialog projects={projects} existing={{ ...started, saving_mode: true }} onClose={() => {}} onStarted={() => {}} onSaved={() => {}} onConversation={() => {}} />,
    );
    await Promise.resolve();
    expect((getByLabelText("Modo ahorro") as HTMLInputElement).checked).toBe(true);
    expect(pref).not.toHaveBeenCalled();
  });
});

describe("NewTaskDialog autopilot", () => {
  const draft = { key: "PROJ-1", title: "Algo", components: [] };
  const mount = () =>
    render(<NewTaskDialog projects={projects} draft={draft} onClose={() => {}} onStarted={() => {}} onSaved={() => {}} onConversation={() => {}} />);

  it("ticking it ticks and disables saving mode; unticking re-enables it, still ticked", async () => {
    vi.spyOn(api, "preference").mockResolvedValue(null);
    const { getByLabelText } = mount();
    const auto = getByLabelText("Autopiloto") as HTMLInputElement;
    const saving = getByLabelText("Modo ahorro") as HTMLInputElement;
    fireEvent.click(auto);
    expect(saving.checked).toBe(true);
    expect(saving.disabled).toBe(true);
    fireEvent.click(auto);
    expect(saving.disabled).toBe(false);
    expect(saving.checked).toBe(true);
  });

  it("is sent with the run together with saving mode", async () => {
    vi.spyOn(api, "preference").mockResolvedValue(null);
    const run = vi.spyOn(api, "run").mockResolvedValueOnce(started);
    const { getByLabelText, container } = mount();
    fireEvent.click(getByLabelText("Autopiloto"));
    fireEvent.submit(container.querySelector("form")!);
    await waitFor(() => expect(run).toHaveBeenCalled());
    expect(run.mock.calls[0][0].autopilot).toBe(true);
    expect(run.mock.calls[0][0].saving_mode).toBe(true);
  });

  it("is sent when saving to the backlog", async () => {
    vi.spyOn(api, "preference").mockResolvedValue(null);
    const create = vi.spyOn(api, "createTask").mockResolvedValueOnce(started);
    const { getByLabelText, getByText } = mount();
    fireEvent.click(getByLabelText("Autopiloto"));
    fireEvent.click(getByText("Guardar en backlog"));
    await waitFor(() => expect(create).toHaveBeenCalled());
    expect(create.mock.calls[0][0]).toMatchObject({ autopilot: true, saving_mode: true });
  });

  it("unticking it on a backlog task that has it sends autopilot: false", async () => {
    vi.spyOn(api, "preference").mockResolvedValue(null);
    const run = vi.spyOn(api, "run").mockResolvedValueOnce(started);
    const existing = { ...started, autopilot: { state: "running" as const, since: "2026-10-21T10:00:00Z" } };
    const { getByLabelText, container } = render(
      <NewTaskDialog projects={projects} existing={existing} onClose={() => {}} onStarted={() => {}} onSaved={() => {}} onConversation={() => {}} />,
    );
    fireEvent.click(getByLabelText("Autopiloto"));
    fireEvent.submit(container.querySelector("form")!);
    await waitFor(() => expect(run).toHaveBeenCalled());
    expect(run.mock.calls[0][0].autopilot).toBe(false);
  });

  it("is hidden outside the spec workflow", () => {
    vi.spyOn(api, "preference").mockResolvedValue(null);
    const { queryByLabelText } = render(
      <NewTaskDialog projects={[{ ...web, workflow: "custom" }]} draft={draft} onClose={() => {}} onStarted={() => {}} onSaved={() => {}} onConversation={() => {}} />,
    );
    expect(queryByLabelText("Autopiloto")).toBeNull();
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
