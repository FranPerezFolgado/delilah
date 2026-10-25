// @vitest-environment jsdom
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { api, ApiError, type Session, type SetupInfo, type Task } from "../api";
import * as platform from "../platform";
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
  title_source: "manual",
  previous_title: null,
  branch_error: null,
  handoff: null,
});
const failed = task({ type: "setup-failed", reason: "npm ERR!\nline 2" });

const info: SetupInfo = {
  command: "npm ci", exit_code: 127, started_at: "2026-01-01T10:00:00Z", finished_at: "2026-01-01T10:00:08Z",
  duration_ms: 8000, cancelled: false, timed_out: false, tail: "npm ERR! missing\nlast line", log_bytes: 900,
};

beforeAll(() => setLanguage("es"));

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("SetupBanner", () => {
  it("falls back to the status reason in a pre block when the daemon has no run", async () => {
    vi.spyOn(api, "setupInfo").mockRejectedValue(new ApiError("none", 404));
    const { container } = render(<SetupBanner task={failed} running={false} onChanged={() => {}} />);
    expect(container.querySelector("pre")!.textContent).toBe("npm ERR!\nline 2");
  });

  it("shows the exit code, duration, command and output tail", async () => {
    vi.spyOn(api, "setupInfo").mockResolvedValue(info);
    const { findByText, container } = render(<SetupBanner task={failed} running={false} onChanged={() => {}} />);
    await findByText("npm ci");
    expect(container.textContent).toContain("Código de salida 127 · 8 s");
    expect(container.querySelector("pre")!.textContent).toBe("npm ERR! missing\nlast line");
    expect(container.querySelector("code")!.className).toContain("mono");
  });

  it("says so when the log is empty", async () => {
    vi.spyOn(api, "setupInfo").mockResolvedValue({ ...info, tail: "", log_bytes: 0 });
    const { findByText, queryByText } = render(<SetupBanner task={failed} running={false} onChanged={() => {}} />);
    await findByText("El setup no escribió nada en su salida.");
    expect(queryByText("Abrir el log completo")).toBeNull();
  });

  it("opens the full log, copies the command and offers the terminal and settings", async () => {
    vi.spyOn(api, "setupInfo").mockResolvedValue(info);
    const ext = vi.spyOn(platform, "openExternal").mockResolvedValue(undefined as never);
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    const term = vi.fn();
    const settings = vi.fn();
    const { findByText } = render(
      <SetupBanner task={failed} running={false} onChanged={() => {}} onOpenTerminal={term} onOpenSettings={settings} />,
    );
    fireEvent.click(await findByText("Abrir el log completo"));
    expect(ext).toHaveBeenCalledWith(api.setupLogUrl("PROJ-1"));
    fireEvent.click(await findByText("Copiar comando"));
    expect(writeText).toHaveBeenCalledWith("npm ci");
    fireEvent.click(await findByText("Abrir terminal aquí"));
    expect(term).toHaveBeenCalled();
    fireEvent.click(await findByText("Comando de setup del proyecto"));
    expect(settings).toHaveBeenCalled();
  });

  it("retry and skip call the api and refresh", async () => {
    vi.spyOn(api, "setupInfo").mockResolvedValue(info);
    const retry = vi.spyOn(api, "setupRetry").mockResolvedValue(failed);
    const skip = vi.spyOn(api, "setupSkip").mockResolvedValue(failed);
    const onChanged = vi.fn();
    const { getByText } = render(<SetupBanner task={failed} running={false} onChanged={onChanged} />);
    fireEvent.click(getByText("Reintentar setup"));
    await waitFor(() => expect(retry).toHaveBeenCalledWith("PROJ-1"));
    fireEvent.click(getByText("Iniciar sin setup"));
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
    vi.spyOn(api, "setupInfo").mockRejectedValue(new ApiError("none", 404));
    const c = render(<SetupBanner task={task({ type: "setup-failed", reason: "cancelled" })} running={false} onChanged={() => {}} />);
    expect(c.container.textContent).toContain("Setup cancelado");
    cleanup();
    const i = render(<SetupBanner task={task({ type: "setup-failed", reason: "interrupted" })} running={false} onChanged={() => {}} />);
    expect(i.container.textContent).toContain("Setup interrumpido: el daemon se reinició");
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
