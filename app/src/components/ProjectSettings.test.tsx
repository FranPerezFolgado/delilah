// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { api, ApiError, type Project } from "../api";
import { setLanguage } from "../i18n";
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
    parent_task_merge: "merge",
  },
};
const parent: Project = { ...repo, name: "mono", kind: "parent" };

beforeEach(() => setLanguage("es"));
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const radios = (c: HTMLElement) => Array.from(c.ownerDocument.querySelectorAll<HTMLElement>("[role=radio][data-name=where]"));
const form = () => document.querySelector("form")!;
const branch = (_c: HTMLElement) => document.querySelector<HTMLInputElement>("input[name=branch]")!;

describe("ProjectSettings", () => {
  it("shows the current settings", () => {
    const { container } = render(<ProjectSettings project={repo} onClose={() => {}} onSaved={() => {}} />);
    expect(branch(container).value).toBe("feat/{ticket}");
    expect(document.querySelector<HTMLInputElement>("input[name=setup]")!.value).toBe("npm ci");
    expect(document.querySelector<HTMLInputElement>("input[name=shared-path-0]")!.value).toBe(".env");
    expect(document.querySelector<HTMLInputElement>("input[name=preview]")).not.toBeNull();
    expect(radios(container).filter((r) => r.getAttribute("aria-checked") === "true").length).toBe(1);
  });

  it("is a real dialog and every field has a help line", () => {
    const { container } = render(<ProjectSettings project={repo} onClose={() => {}} onSaved={() => {}} />);
    expect(document.querySelector("[role=dialog][aria-modal=true]")).not.toBeNull();
    expect(document.querySelector("[role=dialog]")?.textContent).toContain("Dónde trabaja por defecto");
    expect(radios(container).length).toBe(2);
    expect(document.querySelectorAll(".ps-help").length).toBe(7);
  });

  it("the preview command is saved on the project, so saving makes two calls", async () => {
    const put = vi.spyOn(api, "updateProjectSettings").mockResolvedValue(repo);
    const patch = vi.spyOn(api, "updateProject").mockResolvedValue({ ...repo, preview_command: "npm run dev" });
    const onSaved = vi.fn();
    render(<ProjectSettings project={repo} onClose={() => {}} onSaved={onSaved} />);
    fireEvent.change(document.querySelector("input[name=preview]")!, { target: { value: " npm run dev " } });
    fireEvent.submit(form());
    await waitFor(() => expect(onSaved).toHaveBeenCalled());
    expect(put).toHaveBeenCalledTimes(1);
    expect(patch).toHaveBeenCalledWith("web", { preview_command: "npm run dev" });
    expect(onSaved.mock.calls[0][0].preview_command).toBe("npm run dev");
  });

  it("an unchanged preview command makes no second call", async () => {
    const put = vi.spyOn(api, "updateProjectSettings").mockResolvedValue(repo);
    const patch = vi.spyOn(api, "updateProject");
    const onSaved = vi.fn();
    render(<ProjectSettings project={repo} onClose={() => {}} onSaved={onSaved} />);
    fireEvent.submit(form());
    await waitFor(() => expect(onSaved).toHaveBeenCalled());
    expect(put).toHaveBeenCalledTimes(1);
    expect(patch).not.toHaveBeenCalled();
  });

  it("an unset workspace shows the default option and is saved as null untouched", async () => {
    const unset: Project = { ...repo, settings: { ...repo.settings, workspace: null } };
    const put = vi.spyOn(api, "updateProjectSettings").mockResolvedValue(unset);
    const onSaved = vi.fn();
    const onClose = vi.fn();
    const { container } = render(<ProjectSettings project={unset} onClose={onClose} onSaved={onSaved} />);
    const cards = radios(container);
    expect(cards.length).toBe(2);
    expect(cards[0].getAttribute("aria-checked")).toBe("true");
    fireEvent.submit(form());
    await waitFor(() => expect(onSaved).toHaveBeenCalled());
    expect(put).toHaveBeenCalledWith("web", unset.settings);
    expect(put.mock.calls[0][1].workspace).toBeNull();
  });

  it("picking a workspace sets it; the checkout goes back to unset", async () => {
    const put = vi.spyOn(api, "updateProjectSettings").mockResolvedValue(repo);
    const { container } = render(<ProjectSettings project={repo} onClose={() => {}} onSaved={() => {}} />);
    expect(radios(container)[1].getAttribute("aria-checked")).toBe("true");
    fireEvent.click(radios(container)[0]);
    expect(radios(container)[0].getAttribute("aria-checked")).toBe("true");
    fireEvent.submit(form());
    await waitFor(() => expect(put).toHaveBeenCalled());
    expect(put.mock.calls[0][1].workspace).toBeNull();
  });

  it("hides the submodule settings for a repo project, shows them for a parent", async () => {
    vi.spyOn(api, "submodules").mockResolvedValue([{ name: "api", path: "api" }]);
    render(<ProjectSettings project={repo} onClose={() => {}} onSaved={() => {}} />);
    expect(document.querySelector(".picks")).toBeNull();
    cleanup();
    render(<ProjectSettings project={parent} onClose={() => {}} onSaved={() => {}} />);
    await waitFor(() => expect(document.querySelectorAll(".picks input").length).toBe(2));
  });

  it("an empty branch template is saved as null (Delilah proposes the name)", async () => {
    const put = vi.spyOn(api, "updateProjectSettings").mockResolvedValue(repo);
    const onSaved = vi.fn();
    const { container } = render(<ProjectSettings project={repo} onClose={() => {}} onSaved={onSaved} />);
    fireEvent.change(branch(container), { target: { value: "" } });
    expect(branch(container).placeholder).toBeTruthy();
    fireEvent.submit(form());
    await waitFor(() => expect(onSaved).toHaveBeenCalled());
    expect(put).toHaveBeenCalledWith("web", { ...repo.settings, branch: null });
  });

  it("saves the whole settings", async () => {
    const put = vi.spyOn(api, "updateProjectSettings").mockResolvedValue(repo);
    const onSaved = vi.fn();
    const { container } = render(<ProjectSettings project={repo} onClose={() => {}} onSaved={onSaved} />);
    fireEvent.change(branch(container), { target: { value: "x/{ticket}" } });
    fireEvent.submit(form());
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
    render(<ProjectSettings project={repo} onClose={() => {}} onSaved={onSaved} />);
    fireEvent.submit(form());
    await waitFor(() => expect(document.querySelectorAll(".field-error").length).toBe(2));
    expect(document.querySelector("[data-field=branch]")?.textContent).toBe("falta {ticket}");
    expect(document.querySelector("[data-field='shared[0].path']")?.textContent).toBe("fuera del proyecto");
    expect(onSaved).not.toHaveBeenCalled();
  });

  it("asks to discard unsaved edits, and closes straight when nothing changed", () => {
    const onClose = vi.fn();
    const { container } = render(<ProjectSettings project={repo} onClose={onClose} onSaved={() => {}} />);
    fireEvent.change(branch(container), { target: { value: "x/{ticket}" } });
    fireEvent.keyDown(document.querySelector("[role=dialog]")!, { key: "Escape" });
    expect(onClose).not.toHaveBeenCalled();
    expect(document.querySelector("[role=dialog]")?.textContent).toContain("¿Descartar los cambios?");
    fireEvent.click(document.querySelector(".btn-danger")!);
    expect(onClose).toHaveBeenCalledTimes(1);

    cleanup();
    const again = vi.fn();
    render(<ProjectSettings project={repo} onClose={again} onSaved={() => {}} />);
    fireEvent.keyDown(document.querySelector("[role=dialog]")!, { key: "Escape" });
    expect(again).toHaveBeenCalledTimes(1);
  });

  it("keeps submodule rules by path, not name, and renders a stored path checked", async () => {
    vi.spyOn(api, "submodules").mockResolvedValue([{ name: "api", path: "services/api" }, { name: "web", path: "services/web" }]);
    const put = vi.spyOn(api, "updateProjectSettings").mockResolvedValue(parent);
    const p: Project = { ...parent, settings: { ...parent.settings, submodules: { always: [], never: ["services/web"] } } };
    render(<ProjectSettings project={p} onClose={() => {}} onSaved={() => {}} />);
    await waitFor(() => expect(document.querySelectorAll(".picks input").length).toBe(4));
    const boxes = document.querySelectorAll<HTMLInputElement>(".picks input");
    expect(boxes[3].checked).toBe(true);
    fireEvent.click(boxes[0]);
    fireEvent.submit(form());
    await waitFor(() => expect(put).toHaveBeenCalled());
    expect(put.mock.calls[0][1].submodules).toEqual({ always: ["services/api"], never: ["services/web"] });
  });

  it("shows a never submodule as a normal checkbox that can be unticked and re-ticked", async () => {
    const spy = vi.spyOn(api, "submodules").mockResolvedValue([{ name: "web", path: "services/web" }]);
    const put = vi.spyOn(api, "updateProjectSettings").mockResolvedValue(parent);
    const p: Project = { ...parent, settings: { ...parent.settings, submodules: { always: [], never: ["services/web"] } } };
    render(<ProjectSettings project={p} onClose={() => {}} onSaved={() => {}} />);
    await waitFor(() => expect(document.querySelectorAll(".picks input").length).toBe(2));
    expect(spy).toHaveBeenCalledWith(p.name, { all: true });
    expect(document.querySelector("[data-stale]")).toBeNull();
    const never = document.querySelectorAll<HTMLInputElement>(".picks input")[1];
    expect(never.checked).toBe(true);
    fireEvent.click(never);
    expect(never.checked).toBe(false);
    fireEvent.click(never);
    expect(never.checked).toBe(true);
    fireEvent.submit(form());
    await waitFor(() => expect(put).toHaveBeenCalled());
    expect(put.mock.calls[0][1].submodules.never).toEqual(["services/web"]);
  });

  it("lets a stale stored path be unchecked", async () => {
    vi.spyOn(api, "submodules").mockResolvedValue([{ name: "api", path: "api" }]);
    const put = vi.spyOn(api, "updateProjectSettings").mockResolvedValue(parent);
    const p: Project = { ...parent, settings: { ...parent.settings, submodules: { always: ["gone"], never: [] } } };
    render(<ProjectSettings project={p} onClose={() => {}} onSaved={() => {}} />);
    await waitFor(() => expect(document.querySelector("[data-stale=gone]")).not.toBeNull());
    fireEvent.click(document.querySelector("[data-stale=gone] input")!);
    fireEvent.submit(form());
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
    render(<ProjectSettings project={repo} onClose={() => {}} onSaved={() => {}} />);
    fireEvent.submit(form());
    await waitFor(() => expect(document.querySelector(".error:not(.field-error)")).not.toBeNull());
    const banner = document.querySelector(".error:not(.field-error)")!.textContent!;
    expect(banner).toContain("cuerpo inválido");
    expect(banner).toContain("submodules.unknown_key: clave rara");
  });

  it("saves a workflow chosen in the section", async () => {
    vi.spyOn(api, "projectSkills").mockResolvedValue([]);
    const put = vi.spyOn(api, "updateProjectSettings").mockResolvedValue(repo);
    render(<ProjectSettings project={repo} onClose={() => {}} onSaved={() => {}} />);
    fireEvent.change(screen.getByLabelText("Inicio"), { target: { value: "nothing" } });
    fireEvent.submit(form());
    await waitFor(() => expect(put).toHaveBeenCalled());
    expect(put.mock.calls[0][1].workflow).toEqual({ kind: "single", start: { type: "nothing" } });
  });

  it("shows workflow errors next to the field, not in the banner", async () => {
    vi.spyOn(api, "projectSkills").mockResolvedValue([]);
    vi.spyOn(api, "updateProjectSettings").mockRejectedValue(
      new ApiError("invalid", 422, false, undefined, undefined, [{ field: "workflow.start.template", code: "unknown-placeholder", message: "bad {foo}" }]),
    );
    const w: Project = { ...repo, settings: { ...repo.settings, workflow: { kind: "single", start: { type: "message", template: "{foo}" } } } };
    render(<ProjectSettings project={w} onClose={() => {}} onSaved={() => {}} />);
    fireEvent.submit(form());
    await waitFor(() => expect(document.querySelector('[data-field="workflow.start.template"]')).not.toBeNull());
    expect(document.querySelectorAll('[data-field="workflow.start.template"]').length).toBe(1);
  });

  it("the autofix select shows the default and saves the chosen mode", async () => {
    const put = vi.spyOn(api, "updateProjectSettings").mockResolvedValue(repo);
    render(<ProjectSettings project={repo} onClose={() => {}} onSaved={() => {}} />);
    const sel = document.querySelector<HTMLSelectElement>("select[name=ci_autofix]")!;
    expect(sel.value).toBe("autopilot");
    expect(Array.from(sel.options).map((o) => o.value)).toEqual(["off", "autopilot", "always"]);
    fireEvent.change(sel, { target: { value: "always" } });
    fireEvent.submit(form());
    await waitFor(() => expect(put).toHaveBeenCalled());
    expect(put.mock.calls[0][1].ci_autofix).toBe("always");
  });

  it("saving another field keeps ci_autofix as it was", async () => {
    const withMode: Project = { ...repo, settings: { ...repo.settings, ci_autofix: "off" } };
    const put = vi.spyOn(api, "updateProjectSettings").mockResolvedValue(withMode);
    render(<ProjectSettings project={withMode} onClose={() => {}} onSaved={() => {}} />);
    fireEvent.change(document.querySelector("input[name=setup]")!, { target: { value: "npm i" } });
    fireEvent.submit(form());
    await waitFor(() => expect(put).toHaveBeenCalled());
    expect(put.mock.calls[0][1]).toMatchObject({ setup: "npm i", ci_autofix: "off" });
  });
});
