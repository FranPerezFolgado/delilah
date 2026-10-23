// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { api, DEFAULT_PROJECT_SETTINGS, type Project } from "../api";
import { NewTaskDialog } from "./NewTaskDialog";

const project = {
  name: "web",
  kind: "repo",
  settings: { ...DEFAULT_PROJECT_SETTINGS, branch: "feature/{ticket}-{slug}" },
} as unknown as Project;

const parent = {
  name: "mono",
  kind: "parent",
  settings: { ...DEFAULT_PROJECT_SETTINGS, submodules: { always: ["svc-a"], never: ["svc-c"] } },
} as unknown as Project;
const sub = (path: string) => ({ name: path, path }) as never;

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("NewTaskDialog", () => {
  const field = (c: HTMLElement) => c.querySelector('input[aria-label="Branch"]') as HTMLInputElement;

  it("fills the branch field from the template as the title is typed", () => {
    vi.spyOn(api, "proposeBranch").mockReturnValue(new Promise(() => {}));
    const { container } = render(
      <NewTaskDialog projects={[project]} onClose={vi.fn()} onStarted={vi.fn()} onSaved={vi.fn()} onConversation={vi.fn()} />,
    );
    const [ticket, input] = Array.from(container.querySelectorAll("input[required]")) as HTMLInputElement[];
    fireEvent.change(ticket, { target: { value: "PROJ-1" } });
    fireEvent.change(input, { target: { value: "Probar" } });
    expect(field(container).value).toBe("PROJ-1-probar");
    fireEvent.change(input, { target: { value: "Probar caché" } });
    expect(field(container).value).toBe("PROJ-1-probar-cache");
  });

  it("uses LIBRE-? for a free task and shows the full branch for a template without feature/", () => {
    vi.spyOn(api, "proposeBranch").mockReturnValue(new Promise(() => {}));
    const fix = { ...project, settings: { ...project.settings, branch: "fix/{ticket}-{slug}" } } as Project;
    const { container } = render(
      <NewTaskDialog projects={[fix]} onClose={vi.fn()} onStarted={vi.fn()} onSaved={vi.fn()} onConversation={vi.fn()} />,
    );
    const [ticket, input] = Array.from(container.querySelectorAll("input[required]")) as HTMLInputElement[];
    fireEvent.change(ticket, { target: { value: "PROJ-1" } });
    fireEvent.change(input, { target: { value: "Probar" } });
    expect(field(container).value).toBe("fix/PROJ-1-probar");
    expect(container.textContent).not.toContain("feature/");
    fireEvent.click(container.querySelectorAll('button[role="radio"]')[1]);
    expect(field(container).value).toBe("fix/LIBRE-?-probar");
  });

  it("replaces the guess with the daemon's proposal and sends it on start", async () => {
    vi.spyOn(api, "proposeBranch").mockResolvedValue({ branch: "feature/PROJ-1-retry-payments", source: "claude" });
    const run = vi.spyOn(api, "run").mockResolvedValue({} as never);
    const { container } = render(
      <NewTaskDialog projects={[project]} onClose={vi.fn()} onStarted={vi.fn()} onSaved={vi.fn()} onConversation={vi.fn()} />,
    );
    const [ticket, input] = Array.from(container.querySelectorAll("input[required]")) as HTMLInputElement[];
    fireEvent.change(ticket, { target: { value: "PROJ-1" } });
    fireEvent.change(input, { target: { value: "Probar" } });
    await waitFor(() => expect(field(container).value).toBe("PROJ-1-retry-payments"), { timeout: 3000 });
    fireEvent.submit(container.querySelector("form")!);
    await waitFor(() => expect(run).toHaveBeenCalled());
    expect(run.mock.calls[0][0].branch).toBe("feature/PROJ-1-retry-payments");
  });

  it("does not send the guess if the proposal has not arrived", async () => {
    vi.spyOn(api, "proposeBranch").mockReturnValue(new Promise(() => {}));
    const run = vi.spyOn(api, "run").mockResolvedValue({} as never);
    const { container } = render(
      <NewTaskDialog projects={[project]} onClose={vi.fn()} onStarted={vi.fn()} onSaved={vi.fn()} onConversation={vi.fn()} />,
    );
    const [ticket, input] = Array.from(container.querySelectorAll("input[required]")) as HTMLInputElement[];
    fireEvent.change(ticket, { target: { value: "PROJ-1" } });
    fireEvent.change(input, { target: { value: "Probar" } });
    fireEvent.submit(container.querySelector("form")!);
    await waitFor(() => expect(run).toHaveBeenCalled());
    expect(run.mock.calls[0][0].branch).toBeUndefined();
  });

  describe("default workspace mode", () => {
    const withMode = (name: string, workspace: "checkout" | "worktree" | null) =>
      ({ name, kind: "repo", settings: { ...DEFAULT_PROJECT_SETTINGS, workspace } }) as unknown as Project;
    const radio = (c: HTMLElement, v: string) =>
      Array.from(c.querySelectorAll('input[name="where"]'))[v === "checkout" ? 0 : 1] as HTMLInputElement;
    const open = (projects: Project[], extra: Record<string, unknown> = {}) =>
      render(<NewTaskDialog projects={projects} onClose={vi.fn()} onStarted={vi.fn()} onSaved={vi.fn()} onConversation={vi.fn()} {...extra} />);

    it("preselects worktree when the project defaults to it", () => {
      const { container } = open([withMode("a", "worktree")]);
      expect(radio(container, "worktree").checked).toBe(true);
      expect(radio(container, "checkout").checked).toBe(false);
    });

    it("preselects checkout when the project defaults to it", () => {
      const { container } = open([withMode("a", "checkout")]);
      expect(radio(container, "checkout").checked).toBe(true);
    });

    it("preselects checkout (as before) when the project has no default", () => {
      const { container } = open([withMode("a", null)]);
      expect(radio(container, "checkout").checked).toBe(true);
    });

    it("a backlog task with no saved choice and no project default shows a worktree (what the daemon does)", () => {
      const existing = { key: "LIBRE-1", title: "t", project: "a", in_checkout: null, notes: "" } as never;
      const { container } = open([withMode("a", null)], { existing });
      expect(radio(container, "worktree").checked).toBe(true);
    });

    it("re-applies the default when the project changes and the picker is untouched", () => {
      const { container } = open([withMode("a", "checkout"), withMode("b", "worktree")]);
      fireEvent.change(container.querySelector("select") as HTMLSelectElement, { target: { value: "b" } });
      expect(radio(container, "worktree").checked).toBe(true);
    });

    it("keeps a user choice across a title edit and a project change", () => {
      const { container } = open([withMode("a", "checkout"), withMode("b", "worktree")]);
      fireEvent.click(radio(container, "worktree"));
      const [ticket] = Array.from(container.querySelectorAll("input[required]")) as HTMLInputElement[];
      fireEvent.change(ticket, { target: { value: "P-1" } });
      expect(radio(container, "worktree").checked).toBe(true);
      fireEvent.change(container.querySelector("select") as HTMLSelectElement, { target: { value: "a" } });
      expect(radio(container, "worktree").checked).toBe(true);
    });

    it("a backlog task's saved choice wins over the default", () => {
      const existing = { key: "LIBRE-1", title: "t", project: "a", in_checkout: true, notes: "" } as never;
      const { container } = open([withMode("a", "worktree")], { existing });
      expect(radio(container, "checkout").checked).toBe(true);
    });

    it("a backlog task with no saved choice takes the project's default and follows project changes", () => {
      const existing = { key: "LIBRE-1", title: "t", project: "a", in_checkout: null, notes: "" } as never;
      const { container } = open([withMode("a", "worktree"), withMode("b", "checkout")], { existing });
      expect(radio(container, "worktree").checked).toBe(true);
      fireEvent.change(container.querySelector("select") as HTMLSelectElement, { target: { value: "b" } });
      expect(radio(container, "checkout").checked).toBe(true);
    });
  });

  describe("always/never submodules", () => {
    const open = async () => {
      vi.spyOn(api, "submodules").mockResolvedValue([sub("svc-a"), sub("svc-b"), sub("svc-c")]);
      const onStarted = vi.fn();
      const r = render(<NewTaskDialog projects={[parent]} onClose={vi.fn()} onStarted={onStarted} onSaved={vi.fn()} onConversation={vi.fn()} />);
      await waitFor(() => expect(r.container.querySelector('input[type="checkbox"]')).not.toBeNull());
      return r;
    };
    const box = (c: HTMLElement, path: string) =>
      Array.from(c.querySelectorAll("label.choice")).find((l) => l.textContent === path)?.querySelector("input") as HTMLInputElement | undefined;

    it("shows always checked and disabled, and never not listed", async () => {
      const { container } = await open();
      expect(box(container, "svc-a")?.checked).toBe(true);
      expect(box(container, "svc-a")?.disabled).toBe(true);
      expect(box(container, "svc-b")?.checked).toBe(false);
      expect(box(container, "svc-c")).toBeUndefined();
    });

    it("keeps always and drops never in suggestions", async () => {
      vi.spyOn(api, "suggestSubmodules").mockResolvedValue({
        submodules: [{ path: "svc-b", reason: "x" }, { path: "svc-c", reason: "y" }],
        model: "m",
      } as never);
      const { container, getByText } = await open();
      const [ticket, title] = Array.from(container.querySelectorAll("input[required]")) as HTMLInputElement[];
      fireEvent.change(ticket, { target: { value: "P-1" } });
      fireEvent.change(title, { target: { value: "T" } });
      fireEvent.click(getByText("Suggest with Claude"));
      await waitFor(() => expect(box(container, "svc-b")?.checked).toBe(true));
      expect(box(container, "svc-a")?.checked).toBe(true);
      expect(box(container, "svc-c")).toBeUndefined();
    });

    it("sends always on start even if untouched", async () => {
      const run = vi.spyOn(api, "run").mockResolvedValue({} as never);
      const { container } = await open();
      const [ticket, title] = Array.from(container.querySelectorAll("input[required]")) as HTMLInputElement[];
      fireEvent.change(ticket, { target: { value: "P-1" } });
      fireEvent.change(title, { target: { value: "T" } });
      fireEvent.submit(container.querySelector("form")!);
      await waitFor(() => expect(run).toHaveBeenCalled());
      expect(run.mock.calls[0][0].submodules).toEqual(["svc-a"]);
    });
  });
});
