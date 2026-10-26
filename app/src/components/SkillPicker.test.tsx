// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { api, type Skill } from "../api";
import { setLanguage } from "../i18n";
import { SkillPicker, useSkills } from "./SkillPicker";

const skills: Skill[] = [
  { name: "fix", description: "Fix a bug from a ticket", source: "project" },
  { name: "feature", description: "Spec-driven", source: "user" },
  { name: "feature", description: "Plugin feature", source: "plugin", plugin: "kit" },
  { name: "frontend-design:frontend-design", description: "Design UIs", source: "plugin", plugin: "frontend-design" },
];

beforeEach(() => setLanguage("en"));
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("SkillPicker", () => {
  it("lists skills with their source, the same name once per source", async () => {
    vi.spyOn(api, "projectSkills").mockResolvedValue(skills);
    render(<SkillPicker skills={skills} value="" onChange={() => {}} />);
    await screen.findByText("/fix");
    expect(screen.getAllByText("/feature")).toHaveLength(2);
    expect(screen.getByText("/frontend-design:frontend-design")).toBeTruthy();
    expect(screen.getByText("project")).toBeTruthy();
    expect(screen.getByText("user")).toBeTruthy();
    expect(screen.getAllByText("plugin")).toHaveLength(2);
  });

  it("filters by the search text", async () => {
    vi.spyOn(api, "projectSkills").mockResolvedValue(skills);
    render(<SkillPicker skills={skills} value="" onChange={() => {}} />);
    await screen.findByText("/fix");
    fireEvent.change(screen.getByRole("searchbox"), { target: { value: "design" } });
    expect(screen.queryByText("/fix")).toBeNull();
    expect(screen.getByText("/frontend-design:frontend-design")).toBeTruthy();
  });

  it("picking fix fills /fix {ticket}", async () => {
    vi.spyOn(api, "projectSkills").mockResolvedValue(skills);
    const onChange = vi.fn();
    render(<SkillPicker skills={skills} value="" onChange={onChange} />);
    fireEvent.click((await screen.findByText("/fix")).closest("button")!);
    expect(onChange).toHaveBeenCalledWith("/fix {ticket}");
  });

  it("keeps the field editable with free text", async () => {
    vi.spyOn(api, "projectSkills").mockResolvedValue(skills);
    const onChange = vi.fn();
    render(<SkillPicker skills={skills} value="/x" onChange={onChange} />);
    fireEvent.change(screen.getByRole("textbox", { name: "Command" }), { target: { value: "/unknown {ticket}" } });
    expect(onChange).toHaveBeenCalledWith("/unknown {ticket}");
  });

  it("still allows free text when the list is empty or missing", () => {
    const onChange = vi.fn();
    render(<SkillPicker skills={null} value="" onChange={onChange} />);
    expect(screen.getByText("No skills found")).toBeTruthy();
    fireEvent.change(screen.getByRole("textbox", { name: "Command" }), { target: { value: "/a" } });
    expect(onChange).toHaveBeenCalledWith("/a");
  });
});

describe("useSkills", () => {
  const Probe = ({ project }: { project: string }) => <p>{JSON.stringify(useSkills(project)?.map((s) => s.name) ?? "none")}</p>;

  it("fetches once and gives the list; null while loading", async () => {
    const spy = vi.spyOn(api, "projectSkills").mockResolvedValue(skills.slice(0, 1));
    render(<Probe project="web" />);
    expect(screen.getByText('"none"')).toBeTruthy();
    await screen.findByText('["fix"]');
    expect(spy).toHaveBeenCalledTimes(1);
  });

  it("is null when the request fails", async () => {
    const spy = vi.spyOn(api, "projectSkills").mockRejectedValue(new Error("boom"));
    render(<Probe project="web" />);
    await waitFor(() => expect(spy).toHaveBeenCalled());
    expect(screen.getByText('"none"')).toBeTruthy();
  });

  it("asks nothing while no project is chosen", () => {
    const spy = vi.spyOn(api, "projectSkills").mockResolvedValue(skills);
    render(<Probe project="" />);
    expect(spy).not.toHaveBeenCalled();
    expect(screen.getByText('"none"')).toBeTruthy();
  });
});
