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
  it("previews the branch as the title is typed", () => {
    const { container } = render(
      <NewTaskDialog projects={[project]} onClose={vi.fn()} onStarted={vi.fn()} onSaved={vi.fn()} />,
    );
    const [ticket, input] = Array.from(container.querySelectorAll("input[required]")) as HTMLInputElement[];
    fireEvent.change(ticket, { target: { value: "PROJ-1" } });
    fireEvent.change(input, { target: { value: "Probar" } });
    expect(container.textContent).toContain("Branch: feature/PROJ-1-probar");
    fireEvent.change(input, { target: { value: "Probar caché" } });
    expect(container.textContent).toContain("Branch: feature/PROJ-1-probar-cache");
  });

  it("previews LIBRE-? for a free task even after a Jira key was typed", () => {
    const { container } = render(
      <NewTaskDialog projects={[project]} onClose={vi.fn()} onStarted={vi.fn()} onSaved={vi.fn()} />,
    );
    const [ticket, input] = Array.from(container.querySelectorAll("input[required]")) as HTMLInputElement[];
    fireEvent.change(ticket, { target: { value: "PROJ-1" } });
    fireEvent.change(input, { target: { value: "Probar" } });
    expect(container.textContent).toContain("Branch: feature/PROJ-1-probar");
    const radios = container.querySelectorAll('button[role="radio"]');
    fireEvent.click(radios[1]);
    expect(container.textContent).toContain("Branch: feature/LIBRE-?-probar");
    expect(container.textContent).not.toContain("PROJ-1");
  });

  describe("default workspace mode", () => {
    const withMode = (name: string, workspace: "checkout" | "worktree") =>
      ({ name, kind: "repo", settings: { ...DEFAULT_PROJECT_SETTINGS, workspace } }) as unknown as Project;
    const radio = (c: HTMLElement, v: string) =>
      Array.from(c.querySelectorAll('input[name="where"]'))[v === "checkout" ? 0 : 1] as HTMLInputElement;
    const open = (projects: Project[], extra: Record<string, unknown> = {}) =>
      render(<NewTaskDialog projects={projects} onClose={vi.fn()} onStarted={vi.fn()} onSaved={vi.fn()} {...extra} />);

    it("preselects worktree when the project defaults to it", () => {
      const { container } = open([withMode("a", "worktree")]);
      expect(radio(container, "worktree").checked).toBe(true);
      expect(radio(container, "checkout").checked).toBe(false);
    });

    it("preselects checkout when the project defaults to it", () => {
      const { container } = open([withMode("a", "checkout")]);
      expect(radio(container, "checkout").checked).toBe(true);
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
      const r = render(<NewTaskDialog projects={[parent]} onClose={vi.fn()} onStarted={onStarted} onSaved={vi.fn()} />);
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
