// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { setLanguage } from "../i18n";
import { DEFAULT_PROJECT_SETTINGS, api, type Project } from "../api";
import { AddProjectDialog } from "./AddProjectDialog";

beforeEach(() => setLanguage("es"));
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const added: Project = { name: "mi", path: "/p/mi", kind: "repo", workflow: "feature", settings: DEFAULT_PROJECT_SETTINGS };

describe("AddProjectDialog", () => {
  it("is a dialog with folder, name and Repo / Multi-repo cards", () => {
    const { getByRole, getByLabelText } = render(<AddProjectDialog onClose={() => {}} onAdded={() => {}} />);
    expect(getByRole("dialog")).toBeTruthy();
    expect(getByLabelText("Carpeta")).toBeTruthy();
    expect(getByLabelText("Nombre")).toBeTruthy();
    expect(getByRole("radio", { name: /^Repo/ }).getAttribute("aria-checked")).toBe("true");
    expect(getByRole("radio", { name: /^Multi-repo/ })).toBeTruthy();
  });

  it("fills name and kind from the folder and adds", async () => {
    vi.spyOn(api, "inspectFolder").mockResolvedValue({ path: "/p/mi", name: "mi", kind: "parent", submodules: 2 } as never);
    const add = vi.spyOn(api, "addProject").mockResolvedValue(added);
    const onAdded = vi.fn();
    const { getByLabelText, getByRole } = render(<AddProjectDialog onClose={() => {}} onAdded={onAdded} />);
    const path = getByLabelText("Carpeta") as HTMLInputElement;
    fireEvent.change(path, { target: { value: "/p/mi" } });
    fireEvent.blur(path);
    await waitFor(() => expect((getByLabelText("Nombre") as HTMLInputElement).value).toBe("mi"));
    expect(getByRole("radio", { name: /^Multi-repo/ }).getAttribute("aria-checked")).toBe("true");
    fireEvent.click(getByRole("button", { name: "Añadir" }));
    await waitFor(() => expect(add).toHaveBeenCalledWith({ path: "/p/mi", kind: "parent", name: "mi" }));
    await waitFor(() => expect(onAdded).toHaveBeenCalledWith("mi"));
  });

  it("shows «Esa carpeta no es un repositorio git» inline", async () => {
    vi.spyOn(api, "inspectFolder").mockRejectedValue(new Error("not a git repo: /tmp/x"));
    const { getByLabelText, findByRole } = render(<AddProjectDialog onClose={() => {}} onAdded={() => {}} />);
    const path = getByLabelText("Carpeta");
    fireEvent.change(path, { target: { value: "/tmp/x" } });
    fireEvent.blur(path);
    expect((await findByRole("alert")).textContent).toBe("Esa carpeta no es un repositorio git");
  });

  it("closes with Cancelar", () => {
    const onClose = vi.fn();
    const { getByRole } = render(<AddProjectDialog onClose={onClose} onAdded={() => {}} />);
    fireEvent.click(getByRole("button", { name: "Cancelar" }));
    expect(onClose).toHaveBeenCalled();
  });
});
