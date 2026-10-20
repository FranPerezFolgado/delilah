// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { api, ApiError, type Task } from "../api";
import { DeleteTaskDialog } from "./DeleteTaskDialog";

const running: Task = {
  key: "PROJ-1",
  title: "Reintentos",
  project: "web",
  workflow: "feature",
  phase: null,
  status: { type: "running" },
};

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
    const { container } = render(<DeleteTaskDialog task={running} onClose={() => {}} onDeleted={onDeleted} />);

    fireEvent.click(container.querySelector(".btn-danger")!);
    await waitFor(() => expect(container.querySelector(".error")?.textContent).toContain("would lose work"));
    expect(del).toHaveBeenLastCalledWith("PROJ-1", false);
    expect(onDeleted).not.toHaveBeenCalled();

    fireEvent.click(container.querySelector(".btn-danger")!);
    await waitFor(() => expect(onDeleted).toHaveBeenCalledWith("PROJ-1"));
    expect(del).toHaveBeenLastCalledWith("PROJ-1", true);
  });

  it("does not offer forcing for any other error", async () => {
    vi.spyOn(api, "deleteTask").mockRejectedValueOnce(new ApiError("is preparing its workspace", 409));
    const { container } = render(<DeleteTaskDialog task={running} onClose={() => {}} onDeleted={() => {}} />);
    const button = container.querySelector(".btn-danger")!;
    const label = button.textContent;
    fireEvent.click(button);
    await waitFor(() => expect(container.querySelector(".error")).not.toBeNull());
    expect(container.querySelector(".btn-danger")!.textContent).toBe(label);
  });
});
