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

  it("an empty branch template is saved as null (Delilah proposes the name)", async () => {
    const put = vi.spyOn(api, "updateProjectSettings").mockResolvedValue(repo);
    const onSaved = vi.fn();
    const { container } = render(<ProjectSettings project={repo} onClose={() => {}} onSaved={onSaved} />);
    fireEvent.change(branch(container), { target: { value: "" } });
    expect(branch(container).placeholder).toBeTruthy();
    fireEvent.submit(container.querySelector("form")!);
    await waitFor(() => expect(onSaved).toHaveBeenCalled());
    expect(put).toHaveBeenCalledWith("web", { ...repo.settings, branch: null });
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
      new ApiError("invalid", 422, false, undefined, undefined, [
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

  it("keeps submodule rules by path, not name, and renders a stored path checked", async () => {
    vi.spyOn(api, "submodules").mockResolvedValue([{ name: "api", path: "services/api" }, { name: "web", path: "services/web" }]);
    const put = vi.spyOn(api, "updateProjectSettings").mockResolvedValue(parent);
    const p: Project = { ...parent, settings: { ...parent.settings, submodules: { always: [], never: ["services/web"] } } };
    const { container } = render(<ProjectSettings project={p} onClose={() => {}} onSaved={() => {}} />);
    await waitFor(() => expect(container.querySelectorAll(".picks input").length).toBe(4));
    const boxes = container.querySelectorAll<HTMLInputElement>(".picks input");
    expect(boxes[3].checked).toBe(true);
    fireEvent.click(boxes[0]);
    fireEvent.submit(container.querySelector("form")!);
    await waitFor(() => expect(put).toHaveBeenCalled());
    expect(put.mock.calls[0][1].submodules).toEqual({ always: ["services/api"], never: ["services/web"] });
  });

  it("shows a never submodule as a normal checkbox that can be unticked and re-ticked", async () => {
    const spy = vi.spyOn(api, "submodules").mockResolvedValue([{ name: "web", path: "services/web" }]);
    const put = vi.spyOn(api, "updateProjectSettings").mockResolvedValue(parent);
    const p: Project = { ...parent, settings: { ...parent.settings, submodules: { always: [], never: ["services/web"] } } };
    const { container } = render(<ProjectSettings project={p} onClose={() => {}} onSaved={() => {}} />);
    await waitFor(() => expect(container.querySelectorAll(".picks input").length).toBe(2));
    expect(spy).toHaveBeenCalledWith(p.name, { all: true });
    expect(container.querySelector("[data-stale]")).toBeNull();
    const never = container.querySelectorAll<HTMLInputElement>(".picks input")[1];
    expect(never.checked).toBe(true);
    fireEvent.click(never);
    expect(never.checked).toBe(false);
    fireEvent.click(never);
    expect(never.checked).toBe(true);
    fireEvent.submit(container.querySelector("form")!);
    await waitFor(() => expect(put).toHaveBeenCalled());
    expect(put.mock.calls[0][1].submodules.never).toEqual(["services/web"]);
  });

  it("lets a stale stored path be unchecked", async () => {
    vi.spyOn(api, "submodules").mockResolvedValue([{ name: "api", path: "api" }]);
    const put = vi.spyOn(api, "updateProjectSettings").mockResolvedValue(parent);
    const p: Project = { ...parent, settings: { ...parent.settings, submodules: { always: ["gone"], never: [] } } };
    const { container } = render(<ProjectSettings project={p} onClose={() => {}} onSaved={() => {}} />);
    await waitFor(() => expect(container.querySelector("[data-stale=gone]")).not.toBeNull());
    fireEvent.click(container.querySelector("[data-stale=gone] input")!);
    fireEvent.submit(container.querySelector("form")!);
    await waitFor(() => expect(put).toHaveBeenCalled());
    expect(put.mock.calls[0][1].submodules.always).toEqual([]);
  });

  it("shows field errors it cannot place in the general banner", async () => {
    vi.spyOn(api, "updateProjectSettings").mockRejectedValue(
      new ApiError("invalid", 422, false, undefined, undefined, [
        { field: "", code: "settings.invalid", message: "cuerpo inválido" },
        { field: "submodules.unknown_key", code: "settings.invalid", message: "clave rara" },
      ]),
    );
    const { container } = render(<ProjectSettings project={repo} onClose={() => {}} onSaved={() => {}} />);
    fireEvent.submit(container.querySelector("form")!);
    await waitFor(() => expect(container.querySelector(".error:not(.field-error)")).not.toBeNull());
    const banner = container.querySelector(".error:not(.field-error)")!.textContent!;
    expect(banner).toContain("cuerpo inválido");
    expect(banner).toContain("submodules.unknown_key: clave rara");
  });
});
