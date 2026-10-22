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
