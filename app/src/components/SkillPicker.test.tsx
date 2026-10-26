// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { api, type Skill } from "../api";
import { setLanguage } from "../i18n";
import { SkillPicker } from "./SkillPicker";

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
    render(<SkillPicker project="web" value="" onChange={() => {}} />);
    await screen.findByText("/fix");
    expect(screen.getAllByText("/feature")).toHaveLength(2);
    expect(screen.getByText("/frontend-design:frontend-design")).toBeTruthy();
    expect(screen.getByText("project")).toBeTruthy();
    expect(screen.getByText("user")).toBeTruthy();
    expect(screen.getAllByText("plugin")).toHaveLength(2);
  });

  it("filters by the search text", async () => {
    vi.spyOn(api, "projectSkills").mockResolvedValue(skills);
    render(<SkillPicker project="web" value="" onChange={() => {}} />);
    await screen.findByText("/fix");
    fireEvent.change(screen.getByRole("searchbox"), { target: { value: "design" } });
    expect(screen.queryByText("/fix")).toBeNull();
    expect(screen.getByText("/frontend-design:frontend-design")).toBeTruthy();
  });

  it("picking fix fills /fix {ticket}", async () => {
    vi.spyOn(api, "projectSkills").mockResolvedValue(skills);
    const onChange = vi.fn();
    render(<SkillPicker project="web" value="" onChange={onChange} />);
    fireEvent.click((await screen.findByText("/fix")).closest("button")!);
    expect(onChange).toHaveBeenCalledWith("/fix {ticket}");
  });

  it("keeps the field editable with free text", async () => {
    vi.spyOn(api, "projectSkills").mockResolvedValue(skills);
    const onChange = vi.fn();
    render(<SkillPicker project="web" value="/x" onChange={onChange} />);
    fireEvent.change(screen.getByRole("textbox", { name: "Command" }), { target: { value: "/unknown {ticket}" } });
    expect(onChange).toHaveBeenCalledWith("/unknown {ticket}");
  });

  it("still allows free text when the list is empty or fails", async () => {
    const spy = vi.spyOn(api, "projectSkills").mockRejectedValue(new Error("boom"));
    const onChange = vi.fn();
    render(<SkillPicker project="web" value="" onChange={onChange} />);
    await waitFor(() => expect(spy).toHaveBeenCalled());
    await screen.findByText("No skills found");
    fireEvent.change(screen.getByRole("textbox", { name: "Command" }), { target: { value: "/a" } });
    expect(onChange).toHaveBeenCalledWith("/a");
  });
});
