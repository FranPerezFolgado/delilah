// @vitest-environment jsdom
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { api, ApiError, type Session, type Task } from "../api";
import { setLanguage } from "../i18n";
import { SetupBanner } from "./SetupBanner";
import { canArchive } from "./TaskView";
import { isSetupRunning } from "./sessionLabels";

const task = (status: Task["status"]): Task => ({
  key: "PROJ-1",
  title: "T",
  project: "web",
  workflow: "feature",
  phase: null,
  status,
  saving_mode: false,
  handoff: null,
});
const failed = task({ type: "setup-failed", reason: "npm ERR!\nline 2" });

beforeAll(() => setLanguage("es"));

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("SetupBanner", () => {
  it("shows the multi-line reason in a pre block", () => {
    const { container } = render(<SetupBanner task={failed} running={false} onChanged={() => {}} />);
    expect(container.querySelector("pre")!.textContent).toBe("npm ERR!\nline 2");
  });

  it("retry and skip call the api and refresh", async () => {
    const retry = vi.spyOn(api, "setupRetry").mockResolvedValue(failed);
    const skip = vi.spyOn(api, "setupSkip").mockResolvedValue(failed);
    const onChanged = vi.fn();
    const { getByText } = render(<SetupBanner task={failed} running={false} onChanged={onChanged} />);
    fireEvent.click(getByText("Reintentar setup"));
    await waitFor(() => expect(retry).toHaveBeenCalledWith("PROJ-1"));
    fireEvent.click(getByText("Lanzar sin setup"));
    await waitFor(() => expect(skip).toHaveBeenCalledWith("PROJ-1"));
    expect(onChanged).toHaveBeenCalledTimes(2);
  });

  it("offers cancel while the setup runs, and shows a 409", async () => {
    const cancel = vi.spyOn(api, "setupCancel").mockRejectedValue(new ApiError("no setup running", 409, true));
    const { getByText, queryByText, findByText } = render(
      <SetupBanner task={task({ type: "preparing-workspace" })} running onChanged={() => {}} />,
    );
    expect(queryByText("Reintentar setup")).toBeNull();
    fireEvent.click(getByText("Cancelar setup"));
    await waitFor(() => expect(cancel).toHaveBeenCalledWith("PROJ-1"));
    expect(await findByText(/no setup running/)).toBeTruthy();
  });

  it("translates the stable cancelled/interrupted reasons", () => {
    const c = render(<SetupBanner task={task({ type: "setup-failed", reason: "cancelled" })} running={false} onChanged={() => {}} />);
    expect(c.container.querySelector("pre")!.textContent).toBe("Setup cancelado");
    cleanup();
    const i = render(<SetupBanner task={task({ type: "setup-failed", reason: "interrupted" })} running={false} onChanged={() => {}} />);
    expect(i.container.querySelector("pre")!.textContent).toBe("Setup interrumpido: el daemon se reinició");
  });

  it("canArchive covers done, failed and setup-failed only", () => {
    expect(canArchive({ type: "setup-failed", reason: "x" })).toBe(true);
    expect(canArchive({ type: "failed", reason: "x" } as Task["status"])).toBe(true);
    expect(canArchive({ type: "running" })).toBe(false);
  });

  it("isSetupRunning needs a live setup session", () => {
    const s = (kind: string, live: boolean) => ({ kind, live }) as Session;
    expect(isSetupRunning([s("claude", true), s("setup", true)])).toBe(true);
    expect(isSetupRunning([s("setup", false)])).toBe(false);
  });

  it("renders nothing otherwise", () => {
    const { container } = render(<SetupBanner task={task({ type: "running" })} running={false} onChanged={() => {}} />);
    expect(container.innerHTML).toBe("");
  });
});
