// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { api, ApiError, type Project } from "../api";
import { ProjectSettings } from "./ProjectSettings";

const repo: Project = {
  name: "web",
  path: "/dev/web",
  kind: "repo",
  workflow: "feature",
  settings: {
    workspace: "worktree",
    branch: "feat/{ticket}",
    shared: [{ path: ".env", mode: "copy" }],
    setup: "npm ci",
    submodules: { always: [], never: [] },
  },
};
const parent: Project = { ...repo, name: "mono", kind: "parent" };

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const branch = (c: HTMLElement) => c.querySelector<HTMLInputElement>("input[name=branch]")!;

describe("ProjectSettings", () => {
  it("shows the current settings", () => {
    const { container } = render(<ProjectSettings project={repo} onClose={() => {}} onSaved={() => {}} />);
    expect(branch(container).value).toBe("feat/{ticket}");
    expect(container.querySelector<HTMLInputElement>("input[name=setup]")!.value).toBe("npm ci");
    expect(container.querySelector<HTMLInputElement>("input[name=shared-path-0]")!.value).toBe(".env");
    expect(container.querySelector<HTMLInputElement>("input[type=radio][name=where]:checked")).not.toBeNull();
  });

  it("hides the submodule settings for a repo project, shows them for a parent", async () => {
    vi.spyOn(api, "submodules").mockResolvedValue([{ name: "api", path: "api" }]);
    const a = render(<ProjectSettings project={repo} onClose={() => {}} onSaved={() => {}} />);
    expect(a.container.querySelector(".picks")).toBeNull();
    cleanup();
    const b = render(<ProjectSettings project={parent} onClose={() => {}} onSaved={() => {}} />);
    await waitFor(() => expect(b.container.querySelectorAll(".picks input").length).toBe(2));
  });

  it("saves the whole settings", async () => {
    const put = vi.spyOn(api, "updateProjectSettings").mockResolvedValue(repo);
    const onSaved = vi.fn();
    const { container } = render(<ProjectSettings project={repo} onClose={() => {}} onSaved={onSaved} />);
    fireEvent.change(branch(container), { target: { value: "x/{ticket}" } });
    fireEvent.submit(container.querySelector("form")!);
    await waitFor(() => expect(onSaved).toHaveBeenCalled());
    expect(put).toHaveBeenCalledWith("web", { ...repo.settings, branch: "x/{ticket}" });
  });

  it("shows 422 field errors under their fields", async () => {
    vi.spyOn(api, "updateProjectSettings").mockRejectedValue(
      new ApiError("invalid", 422, false, [
        { field: "branch", code: "x", message: "falta {ticket}" },
        { field: "shared[0].path", code: "y", message: "fuera del proyecto" },
      ]),
    );
    const onSaved = vi.fn();
    const { container } = render(<ProjectSettings project={repo} onClose={() => {}} onSaved={onSaved} />);
    fireEvent.submit(container.querySelector("form")!);
    await waitFor(() => expect(container.querySelectorAll(".field-error").length).toBe(2));
    expect(container.querySelector("[data-field=branch]")?.textContent).toBe("falta {ticket}");
    expect(container.querySelector("[data-field='shared[0].path']")?.textContent).toBe("fuera del proyecto");
    expect(onSaved).not.toHaveBeenCalled();
  });

  it("asks to discard unsaved edits, and closes straight when nothing changed", () => {
    const onClose = vi.fn();
    const { container } = render(<ProjectSettings project={repo} onClose={onClose} onSaved={() => {}} />);
    fireEvent.change(branch(container), { target: { value: "x/{ticket}" } });
    fireEvent.keyDown(window, { key: "Escape" });
    expect(onClose).not.toHaveBeenCalled();
    expect(container.querySelector("[role=alertdialog]")).not.toBeNull();
    fireEvent.click(container.querySelector(".btn-danger")!);
    expect(onClose).toHaveBeenCalledTimes(1);

    cleanup();
    const again = vi.fn();
    render(<ProjectSettings project={repo} onClose={again} onSaved={() => {}} />);
    fireEvent.keyDown(window, { key: "Escape" });
    expect(again).toHaveBeenCalledTimes(1);
  });
});
