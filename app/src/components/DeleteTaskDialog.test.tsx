// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { setLanguage } from "../i18n";
import { api, ApiError, type Task } from "../api";
import { DeleteTaskDialog } from "./DeleteTaskDialog";

const running: Task = {
  created_at: "2026-01-01T00:00:00Z", finished_at: null, key: "PROJ-1",
  title: "Reintentos",
  project: "web",
  workflow: "feature",
  phase: null,
  status: { type: "running" },
  saving_mode: false, title_source: "initial", previous_title: null, branch_error: null,
  handoff: null,
};

beforeEach(() => setLanguage("es"));

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("DeleteTaskDialog", () => {
  it("asks again, to force it, only when work would be lost", async () => {
    const del = vi
      .spyOn(api, "deleteTask")
      .mockRejectedValueOnce(new ApiError("PROJ-1 would lose work", 409, true))
      .mockResolvedValueOnce(null);
    const onDeleted = vi.fn();
    const { getByRole, getByText } = render(<DeleteTaskDialog task={running} onClose={() => {}} onDeleted={onDeleted} />);

    fireEvent.click(getByText("Eliminar tarea"));
    await waitFor(() => expect(getByRole("dialog").textContent).toContain("would lose work"));
    expect(getByText("Eliminar igualmente").className).toContain("btn-danger");
    expect(del).toHaveBeenLastCalledWith("PROJ-1", false);
    expect(onDeleted).not.toHaveBeenCalled();

    fireEvent.click(getByText("Eliminar igualmente"));
    await waitFor(() => expect(onDeleted).toHaveBeenCalledWith("PROJ-1"));
    expect(del).toHaveBeenLastCalledWith("PROJ-1", true);
  });

  it("does not offer forcing for any other error", async () => {
    vi.spyOn(api, "deleteTask").mockRejectedValueOnce(new ApiError("is preparing its workspace", 409));
    const { getByRole, getByText, queryByText } = render(<DeleteTaskDialog task={running} onClose={() => {}} onDeleted={() => {}} />);
    fireEvent.click(getByText("Eliminar tarea"));
    await waitFor(() => expect(getByRole("dialog").textContent).toContain("is preparing its workspace"));
    expect(queryByText("Eliminar igualmente")).toBeNull();
    expect(getByText("Eliminar tarea")).toBeTruthy();
  });

  it("footer is Cancelar then the danger action, and states the consequences", () => {
    const onClose = vi.fn();
    const { getByRole, container } = render(<DeleteTaskDialog task={running} onClose={onClose} onDeleted={() => {}} />);
    const buttons = [...container.querySelectorAll(".dialog-ft button")];
    expect(buttons.map((b) => b.textContent)).toEqual(["Cancelar", "Eliminar tarea"]);
    expect(buttons[1].className).toContain("btn-danger");
    const text = getByRole("dialog").textContent!;
    expect(text).toContain("¿Eliminar PROJ-1?");
    expect(text).toContain("Se parará el agente.");
    fireEvent.click(buttons[0]);
    expect(onClose).toHaveBeenCalled();
  });
});
