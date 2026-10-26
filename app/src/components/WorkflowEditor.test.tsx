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
