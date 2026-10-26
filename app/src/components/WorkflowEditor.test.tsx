// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { api, type ProjectWorkflow, type Skill } from "../api";
import { setLanguage } from "../i18n";
import { WorkflowEditor } from "./WorkflowEditor";

const skills: Skill[] = [{ name: "fix", description: "Fix a bug", source: "project" }];

beforeEach(() => {
  setLanguage("en");
  vi.spyOn(api, "projectSkills").mockResolvedValue(skills);
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const setup = (value: ProjectWorkflow | null | undefined, errors: { field: string; code: string; message: string }[] = []) => {
  const onChange = vi.fn();
  render(<WorkflowEditor project="web" value={value} errors={errors} onChange={onChange} />);
  return onChange;
};

describe("WorkflowEditor", () => {
  it("no workflow means the built-in prompt", () => {
    setup(undefined);
    expect((screen.getByLabelText("Start") as HTMLSelectElement).value).toBe("builtin");
    expect(screen.queryByLabelText("Command")).toBeNull();
  });

  it("switches between built-in, message and nothing", () => {
    const onChange = setup({ kind: "single", start: { type: "builtin" } });
    fireEvent.change(screen.getByLabelText("Start"), { target: { value: "message" } });
    expect(onChange).toHaveBeenLastCalledWith({ kind: "single", start: { type: "message", template: "" } });
    fireEvent.change(screen.getByLabelText("Start"), { target: { value: "nothing" } });
    expect(onChange).toHaveBeenLastCalledWith({ kind: "single", start: { type: "nothing" } });
  });

  it("edits the message and picks a skill", async () => {
    const onChange = setup({ kind: "single", start: { type: "message", template: "" } });
    fireEvent.change(screen.getByLabelText("Command"), { target: { value: "hi {ticket}" } });
    expect(onChange).toHaveBeenLastCalledWith({ kind: "single", start: { type: "message", template: "hi {ticket}" } });
    fireEvent.click(await screen.findByText("/fix"));
    expect(onChange).toHaveBeenLastCalledWith({ kind: "single", start: { type: "message", template: "/fix {ticket}" } });
  });

  it("applying a preset fills the fields", () => {
    const onChange = setup({ kind: "single", start: { type: "builtin" } });
    fireEvent.change(screen.getByLabelText("Preset"), { target: { value: "speckit-feature" } });
    expect(onChange).toHaveBeenLastCalledWith(expect.objectContaining({ kind: "phased", plan: "/feature {task}", implement: "/orchestrate {spec_dir}" }));
    fireEvent.change(screen.getByLabelText("Preset"), { target: { value: "nothing" } });
    expect(onChange).toHaveBeenLastCalledWith({ kind: "single", start: { type: "nothing" } });
  });

  it("shows phased commands as editable text", () => {
    const onChange = setup({ kind: "phased", plan: "/a", implement: "/b", review: "/c" });
    expect((screen.getByLabelText("Plan") as HTMLInputElement).value).toBe("/a");
    fireEvent.change(screen.getByLabelText("Implement"), { target: { value: "/z" } });
    expect(onChange).toHaveBeenLastCalledWith({ kind: "phased", plan: "/a", implement: "/z", review: "/c" });
  });

  it("fetches the skills once for all three phased fields, each with a picker", async () => {
    const spy = vi.spyOn(api, "projectSkills").mockResolvedValue(skills);
    const onChange = setup({ kind: "phased", plan: "/a", implement: "/b", review: "/c" });
    await screen.findAllByText("/fix");
    expect(spy).toHaveBeenCalledTimes(1);
    expect(screen.getAllByText("/fix")).toHaveLength(3);
    fireEvent.click(screen.getAllByText("/fix")[2].closest("button")!);
    expect(onChange).toHaveBeenLastCalledWith({ kind: "phased", plan: "/a", implement: "/b", review: "/fix {ticket}" });
  });

  it("mentions {spec_dir} only on implement, and {ticket}, {title}, {task} everywhere", () => {
    const { container } = render(
      <WorkflowEditor project="web" value={{ kind: "phased", plan: "", implement: "", review: "" }} errors={[]} onChange={() => {}} />,
    );
    const helps = Array.from(container.querySelectorAll(".field .ps-help")).map((e) => e.textContent ?? "");
    expect(helps).toHaveLength(3);
    expect(helps.filter((h) => h.includes("{spec_dir}"))).toHaveLength(1);
    expect(helps.every((h) => h.includes("{ticket}") && h.includes("{title}") && h.includes("{task}"))).toBe(true);
    expect(helps[1]).toContain("{spec_dir}");
  });

  it("shows the blank errors of implement and review next to their fields", () => {
    const { container } = render(
      <WorkflowEditor project="web" value={{ kind: "phased", plan: "", implement: "", review: "" }}
        errors={[{ field: "workflow.implement", code: "workflow.blank", message: "blank" }, { field: "workflow.review", code: "workflow.blank", message: "blank" }]} onChange={() => {}} />,
    );
    expect(container.querySelector('[data-field="workflow.implement"]')).toBeTruthy();
    expect(container.querySelector('[data-field="workflow.review"]')).toBeTruthy();
  });

  it("warns when the skill is not found, and only then", async () => {
    const { rerender } = render(
      <WorkflowEditor project="web" value={{ kind: "single", start: { type: "message", template: "/nope {ticket}" } }} errors={[]} onChange={() => {}} />,
    );
    await screen.findByRole("alert");
    expect(screen.getByRole("alert").textContent).toContain("/nope");
    rerender(<WorkflowEditor project="web" value={{ kind: "single", start: { type: "message", template: "/fix {ticket}" } }} errors={[]} onChange={() => {}} />);
    await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
    rerender(<WorkflowEditor project="web" value={{ kind: "single", start: { type: "message", template: "do it" } }} errors={[]} onChange={() => {}} />);
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("no warning when the skills failed to load", async () => {
    vi.spyOn(api, "projectSkills").mockRejectedValue(new Error("x"));
    setup({ kind: "single", start: { type: "message", template: "/nope" } });
    await new Promise((r) => setTimeout(r, 10));
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("no warning when the skill list is empty", async () => {
    vi.spyOn(api, "projectSkills").mockResolvedValue([]);
    setup({ kind: "single", start: { type: "message", template: "/nope" } });
    await new Promise((r) => setTimeout(r, 10));
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("shows daemon field errors next to their fields", () => {
    const { container } = render(
      <WorkflowEditor
        project="web"
        value={{ kind: "single", start: { type: "message", template: "{foo}" } }}
        errors={[{ field: "workflow.start.template", code: "x", message: "unknown placeholder" }]}
        onChange={() => {}}
      />,
    );
    expect(container.querySelector('[data-field="workflow.start.template"]')?.textContent).toBe("unknown placeholder");
  });

  it("shows phased field errors", () => {
    const { container } = render(
      <WorkflowEditor project="web" value={{ kind: "phased", plan: "", implement: "/b", review: "/c" }}
        errors={[{ field: "workflow.plan", code: "x", message: "empty" }]} onChange={() => {}} />,
    );
    expect(container.querySelector('[data-field="workflow.plan"]')?.textContent).toBe("empty");
  });
});
