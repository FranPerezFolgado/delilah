// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { setLanguage } from "../i18n";
import { api, type PreviewView, type Project } from "../api";
import { PreviewControl } from "./PreviewControl";

const openExternal = vi.hoisted(() => vi.fn());
vi.mock("../platform", async (orig) => ({ ...(await orig<typeof import("../platform")>()), openExternal }));

beforeEach(() => setLanguage("es"));
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  openExternal.mockClear();
});

const proj = (preview_command: string | null): Project => ({ name: "app", path: "/x", kind: "repo", workflow: "feature", preview_command });
const view = (over: Partial<PreviewView>): PreviewView => ({ state: "running", started_at: "2026-01-01T00:00:00Z", ...over });

describe("PreviewControl", () => {
  it("is hidden when the project has no preview command", async () => {
    const projects = vi.spyOn(api, "projects").mockResolvedValue([proj(null)]);
    vi.spyOn(api, "getPreview").mockResolvedValue(null);
    const { container } = render(<PreviewControl taskKey="T-1" project="app" />);
    await waitFor(() => expect(projects).toHaveBeenCalled());
    expect(container.textContent).toBe("");
  });

  it("starts a preview", async () => {
    vi.spyOn(api, "projects").mockResolvedValue([proj("npm run dev")]);
    vi.spyOn(api, "getPreview").mockResolvedValue(null);
    const start = vi.spyOn(api, "startPreview").mockResolvedValue(view({ state: "starting" }));
    const { findByText } = render(<PreviewControl taskKey="T-1" project="app" />);
    fireEvent.click(await findByText("Lanzar preview"));
    await findByText("Preview arrancando");
    expect(start).toHaveBeenCalledWith("T-1");
  });

  it("links the running URL and stops it", async () => {
    vi.spyOn(api, "projects").mockResolvedValue([proj("npm run dev")]);
    vi.spyOn(api, "getPreview").mockResolvedValue(view({ url: "http://localhost:5173" }));
    const stop = vi.spyOn(api, "stopPreview").mockResolvedValue(null);
    const { findByText } = render(<PreviewControl taskKey="T-1" project="app" />);
    fireEvent.click(await findByText("http://localhost:5173"));
    expect(openExternal).toHaveBeenCalledWith("http://localhost:5173");
    fireEvent.click(await findByText("Parar preview"));
    await findByText("Lanzar preview");
    expect(stop).toHaveBeenCalledWith("T-1");
  });

  it("shows a failure with Reintentar", async () => {
    vi.spyOn(api, "projects").mockResolvedValue([proj("npm run dev")]);
    vi.spyOn(api, "getPreview").mockResolvedValue(view({ state: "failed", error: "puerto ocupado" }));
    const start = vi.spyOn(api, "startPreview").mockResolvedValue(view({ state: "starting" }));
    const { findByText } = render(<PreviewControl taskKey="T-1" project="app" />);
    fireEvent.click(await findByText("Reintentar"));
    await findByText("Preview arrancando");
    expect(start).toHaveBeenCalled();
  });

  it("shows the error text of a failed preview", async () => {
    vi.spyOn(api, "projects").mockResolvedValue([proj("npm run dev")]);
    vi.spyOn(api, "getPreview").mockResolvedValue(view({ state: "failed", error: "puerto ocupado" }));
    const { findByText } = render(<PreviewControl taskKey="T-1" project="app" />);
    await findByText(/puerto ocupado/);
  });
});
