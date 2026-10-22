// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/react";
import { DEFAULT_PROJECT_SETTINGS, type Project } from "../api";
import { NewTaskDialog } from "./NewTaskDialog";

const project = {
  name: "web",
  kind: "repo",
  settings: { ...DEFAULT_PROJECT_SETTINGS, branch: "feature/{ticket}-{slug}" },
} as unknown as Project;

afterEach(cleanup);

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
});
