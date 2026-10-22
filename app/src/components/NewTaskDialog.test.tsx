// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render } from "@testing-library/react";
import { api, type BranchProposal, type Project, type Task } from "../api";
import { NewTaskDialog } from "./NewTaskDialog";

const projects: Project[] = [{ name: "app", kind: "repo" } as Project];

beforeEach(() => {
  vi.useFakeTimers();
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.restoreAllMocks();
});

const proposal = (branch: string, source: BranchProposal["source"] = "claude"): BranchProposal => ({ branch, source });

function setup(props: Partial<React.ComponentProps<typeof NewTaskDialog>> = {}) {
  const utils = render(<NewTaskDialog projects={projects} onClose={() => {}} onStarted={() => {}} onSaved={() => {}} {...props} />);
  const radios = utils.container.querySelectorAll("[role=radio]");
  if (radios[1]) fireEvent.click(radios[1]); // free task: no ticket key needed
  const field = () => utils.getByLabelText("Branch") as HTMLInputElement;
  const title = () => utils.container.querySelector("input[required]") as HTMLInputElement;
  return { ...utils, field, title };
}

const type = (el: HTMLInputElement, value: string) => fireEvent.change(el, { target: { value } });
const wait = (ms: number) => act(async () => { await vi.advanceTimersByTimeAsync(ms); });

describe("NewTaskDialog branch field", () => {
  it("proposes after 1 s and shows the source", async () => {
    const spy = vi.spyOn(api, "proposeBranch").mockResolvedValue(proposal("feature/arreglo-del-login"));
    const { field, title, container } = setup();
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
    const { field, title } = setup();
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
    const { field, title, getByText } = setup();
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
    const { field, title } = setup();
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
    const { field, title, container } = setup();
    type(title(), "a");
    await wait(1100);
    expect(field().value).toBe("");
    expect(field().placeholder).toBe("automatic if left empty");
    expect(container.querySelector(".error")).toBeNull();
  });

  it("keeps spaces typed character by character and submits them", async () => {
    vi.spyOn(api, "proposeBranch").mockResolvedValue({ branch: null, source: "title" });
    const run = vi.spyOn(api, "run").mockRejectedValue(new Error("x"));
    const { field, title, getByText } = setup();
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
    const { title, container } = setup();
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
    const { getByText } = setup({ draft: { key: "PROJ-12", title: "Retry failed payments", components: [] } });
    await wait(1100);
    fireEvent.click(getByText("Start", { selector: "button" }));
    await wait(0);
    expect(run.mock.calls[0][0].branch).toBe("feature/PROJ-12-retry-failed-payments");
  });

  it("shows the field for existing and fromSession", () => {
    vi.spyOn(api, "proposeBranch").mockResolvedValue(proposal("feature/x"));
    const existing = { key: "LIBRE-3", title: "T", project: "app", in_checkout: true } as Task;
    expect(setup({ existing }).field()).toBeTruthy();
    cleanup();
    expect(setup({ fromSession: { id: "s", label: "L" }, project: "app" }).field()).toBeTruthy();
  });
});
