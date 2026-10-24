// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { setLanguage } from "../i18n";
import { DEFAULT_PROJECT_SETTINGS, api, type Project } from "../api";
import { ProjectView } from "./ProjectView";

beforeEach(() => setLanguage("es"));
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const project: Project = { name: "app", path: "/x", kind: "repo", workflow: "feature", settings: DEFAULT_PROJECT_SETTINGS, preview_command: "npm run dev" };
const view = (p: Project = project) => render(<ProjectView project={p} sessions={[]} onOpenSession={() => {}} onNewSession={async () => {}} />);

describe("ProjectView preview command", () => {
  it("saves a changed command on blur", async () => {
    const update = vi.spyOn(api, "updateProject").mockResolvedValue(project);
    const { getByLabelText } = view();
    const input = getByLabelText("Comando de preview") as HTMLInputElement;
    expect(input.value).toBe("npm run dev");
    fireEvent.change(input, { target: { value: " pnpm dev " } });
    fireEvent.blur(input);
    await waitFor(() => expect(update).toHaveBeenCalledWith("app", { preview_command: "pnpm dev" }));
  });

  it("clears it when emptied, on Enter", async () => {
    const update = vi.spyOn(api, "updateProject").mockResolvedValue(project);
    const { getByLabelText } = view();
    const input = getByLabelText("Comando de preview");
    fireEvent.change(input, { target: { value: "" } });
    fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => expect(update).toHaveBeenCalledWith("app", { preview_command: null }));
  });

  it("does not save an unchanged value", () => {
    const update = vi.spyOn(api, "updateProject").mockResolvedValue(project);
    const { getByLabelText } = view();
    fireEvent.blur(getByLabelText("Comando de preview"));
    expect(update).not.toHaveBeenCalled();
  });

  it("shows the error when saving fails", async () => {
    vi.spyOn(api, "updateProject").mockRejectedValue(new Error("no se pudo guardar"));
    const { getByLabelText, findByText } = view();
    const input = getByLabelText("Comando de preview");
    fireEvent.change(input, { target: { value: "pnpm dev" } });
    fireEvent.blur(input);
    await findByText("no se pudo guardar");
  });
});
