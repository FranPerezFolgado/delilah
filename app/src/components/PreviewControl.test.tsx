// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { setLanguage } from "../i18n";
import { DEFAULT_PROJECT_SETTINGS, api, ApiError, type PreviewView, type Project } from "../api";
import { PreviewControl } from "./PreviewControl";

const live = vi.hoisted(() => ({ handlers: {} as Record<string, () => void> }));
vi.mock("../live", () => ({
  on: (topic: string, cb: () => void) => {
    live.handlers[topic] = cb;
    return () => {};
  },
}));

const openExternal = vi.hoisted(() => vi.fn());
vi.mock("../platform", async (orig) => ({ ...(await orig<typeof import("../platform")>()), openExternal }));

beforeEach(() => setLanguage("es"));
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  openExternal.mockClear();
});

const proj = (preview_command: string | null): Project => ({ name: "app", path: "/x", kind: "repo", workflow: "feature", settings: DEFAULT_PROJECT_SETTINGS, preview_command });
const view = (over: Partial<PreviewView>): PreviewView => ({ state: "running", started_at: "2026-01-01T00:00:00Z", ...over });

describe("PreviewControl", () => {
  it("is hidden when the project has no preview command", async () => {
    const projects = vi.spyOn(api, "projects").mockResolvedValue([proj(null)]);
    vi.spyOn(api, "getPreview").mockResolvedValue(null);
    const { container } = render(<PreviewControl taskKey="T-1" project="app" />);
    await waitFor(() => expect(projects).toHaveBeenCalled());
    expect(container.textContent).toBe("");
  });

  it("keeps Stop for a running preview after the command is cleared", async () => {
    vi.spyOn(api, "projects").mockResolvedValue([proj(null)]);
    vi.spyOn(api, "getPreview").mockResolvedValue(view({ url: "http://localhost:5173" }));
    const { findByRole } = render(<PreviewControl taskKey="T-1" project="app" />);
    fireEvent.click(await findByRole("button", { name: "Preview :5173" }));
    screen.getByRole("menuitem", { name: "Detener" });
  });

  it("starts a preview", async () => {
    vi.spyOn(api, "projects").mockResolvedValue([proj("npm run dev")]);
    vi.spyOn(api, "getPreview").mockResolvedValue(null);
    const start = vi.spyOn(api, "startPreview").mockResolvedValue(view({ state: "starting" }));
    const { findByRole } = render(<PreviewControl taskKey="T-1" project="app" />);
    fireEvent.click(await findByRole("button", { name: "Preview" }));
    await findByRole("button", { name: "Preview iniciándose" });
    expect(start).toHaveBeenCalledWith("T-1");
  });

  it("shows Preview :port with an ok dot; the menu opens the URL and stops it", async () => {
    vi.spyOn(api, "projects").mockResolvedValue([proj("npm run dev")]);
    vi.spyOn(api, "getPreview").mockResolvedValue(view({ url: "http://localhost:5173" }));
    const stop = vi.spyOn(api, "stopPreview").mockResolvedValue(null);
    const { findByRole, container } = render(<PreviewControl taskKey="T-1" project="app" />);
    const trigger = await findByRole("button", { name: "Preview :5173" });
    expect(container.querySelector(".preview-dot")).not.toBeNull();
    fireEvent.click(trigger);
    fireEvent.click(screen.getByRole("menuitem", { name: "Abrir" }));
    expect(openExternal).toHaveBeenCalledWith("http://localhost:5173");
    fireEvent.click(await findByRole("button", { name: "Preview :5173" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Detener" }));
    await findByRole("button", { name: "Preview" });
    expect(stop).toHaveBeenCalledWith("T-1");
  });

  it("icon-only drops the text but keeps the accessible name", async () => {
    vi.spyOn(api, "projects").mockResolvedValue([proj("npm run dev")]);
    vi.spyOn(api, "getPreview").mockResolvedValue(null);
    const { findByRole } = render(<PreviewControl taskKey="T-1" project="app" iconOnly />);
    const b = await findByRole("button", { name: "Preview" });
    expect(b.textContent).toBe("");
  });

  it("shows a failure with Reintentar", async () => {
    vi.spyOn(api, "projects").mockResolvedValue([proj("npm run dev")]);
    vi.spyOn(api, "getPreview").mockResolvedValue(view({ state: "failed", error: "puerto ocupado" }));
    const start = vi.spyOn(api, "startPreview").mockResolvedValue(view({ state: "starting" }));
    const { findByText, findByRole } = render(<PreviewControl taskKey="T-1" project="app" />);
    fireEvent.click(await findByText("Reintentar"));
    await findByRole("button", { name: "Preview iniciándose" });
    expect(start).toHaveBeenCalled();
  });

  it("shows the error text of a failed preview", async () => {
    vi.spyOn(api, "projects").mockResolvedValue([proj("npm run dev")]);
    vi.spyOn(api, "getPreview").mockResolvedValue(view({ state: "failed", error: "puerto ocupado" }));
    const { findByText } = render(<PreviewControl taskKey="T-1" project="app" />);
    await findByText(/puerto ocupado/);
  });

  it("is hidden without a project", async () => {
    const projects = vi.spyOn(api, "projects").mockResolvedValue([proj("npm run dev")]);
    vi.spyOn(api, "getPreview").mockResolvedValue(null);
    const { container } = render(<PreviewControl taskKey="T-1" project={null} />);
    await Promise.resolve();
    expect(projects).not.toHaveBeenCalled();
    expect(container.textContent).toBe("");
  });

  it("refetches when the tasks topic fires", async () => {
    vi.spyOn(api, "projects").mockResolvedValue([proj("npm run dev")]);
    const get = vi.spyOn(api, "getPreview").mockResolvedValue(null);
    render(<PreviewControl taskKey="T-1" project="app" />);
    await screen.findByRole("button", { name: "Preview" });
    const calls = get.mock.calls.length;
    live.handlers.tasks();
    await waitFor(() => expect(get.mock.calls.length).toBeGreaterThan(calls));
  });

  it("refetches on a 409 running instead of showing an error", async () => {
    vi.spyOn(api, "projects").mockResolvedValue([proj("npm run dev")]);
    const get = vi.spyOn(api, "getPreview").mockResolvedValueOnce(null).mockResolvedValue(view({ url: "http://localhost:1" }));
    vi.spyOn(api, "startPreview").mockRejectedValue(new ApiError("ya en marcha", 409, false, "running"));
    const { findByRole, queryByText } = render(<PreviewControl taskKey="T-1" project="app" />);
    fireEvent.click(await findByRole("button", { name: "Preview" }));
    await findByRole("button", { name: "Preview :1" });
    expect(queryByText("ya en marcha")).toBeNull();
    expect(get).toHaveBeenCalledTimes(2);
  });

  it("is hidden on a done or failed task", async () => {
    const projects = vi.spyOn(api, "projects").mockResolvedValue([proj("npm run dev")]);
    vi.spyOn(api, "getPreview").mockResolvedValue(null);
    for (const status of [{ type: "done" }, { type: "failed", reason: "x" }] as const) {
      const { container, unmount } = render(<PreviewControl taskKey="T-1" project="app" status={status} />);
      await waitFor(() => expect(projects).toHaveBeenCalled());
      expect(container.textContent).toBe("");
      unmount();
    }
  });

  it("explains a task without workspace", async () => {
    vi.spyOn(api, "projects").mockResolvedValue([proj("npm run dev")]);
    vi.spyOn(api, "getPreview").mockResolvedValue(null);
    vi.spyOn(api, "startPreview").mockRejectedValue(new ApiError("the task has no workspace yet", 409, false, "no-workspace"));
    const { findByText, findByRole, queryByText } = render(<PreviewControl taskKey="T-1" project="app" status={{ type: "queued" }} />);
    fireEvent.click(await findByRole("button", { name: "Preview" }));
    await findByText("La tarea aún no tiene espacio de trabajo");
    expect(queryByText(/the task has no workspace/)).toBeNull();
  });
});
