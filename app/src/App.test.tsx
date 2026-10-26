// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { DEFAULT_PROJECT_SETTINGS } from "./api";
import { setLanguage } from "./i18n";

const mocked = vi.hoisted(() => ({ tasks: [] as unknown[], projects: [] as unknown[], deleteTask: vi.fn() }));
const conn = vi.hoisted(() => ({ status: "up" as "up" | "down", lastOk: null as number | null, retry: () => {} }));
vi.mock("./connection", () => ({ useConnection: () => conn }));
vi.mock("./live", () => ({ connect: (cb?: (c: boolean) => void) => { cb?.(false); return () => {}; }, on: () => () => {} }));
vi.mock("./alerts", () => ({ useDesktopAlerts: () => {} }));
vi.mock("./useUpdates", () => ({
  useUpdates: () => ({ state: { phase: "idle" }, dispatch: () => {}, meta: { installed: false }, check: () => {} }),
}));
vi.mock("./components/UpdateNotice", () => ({ UpdateNotice: () => null }));
vi.mock("./api", async (orig) => {
  const real = await orig<typeof import("./api")>();
  const empty = () => Promise.resolve([]);
  return {
    ...real,
    onRequestResult: () => () => {},
    api: {
      ...real.api,
      tasks: () => Promise.resolve(mocked.tasks),
      deleteTask: mocked.deleteTask,
      projects: () => Promise.resolve(mocked.projects),
      sessions: empty,
      inbox: empty,
      prSummaries: empty,
      specs: () => Promise.resolve({}),
      usage: () => Promise.resolve(null),
      preference: () => Promise.resolve(null),
      setPreference: () => Promise.resolve(),
    },
  };
});

import { App } from "./App";

beforeEach(() => {
  setLanguage("es");
  localStorage.clear();
  conn.status = "up";
  mocked.tasks = [];
  mocked.projects = [];
  mocked.deleteTask.mockReset();
});
afterEach(cleanup);

const mount = async () => {
  render(<App />);
  await act(async () => {});
};

describe("App shell", () => {
  it("shows the breadcrumb with drag regions and follows the view", async () => {
    await mount();
    const crumbs = document.querySelector(".crumbs")!;
    expect(crumbs.textContent).toContain("Delilah");
    expect(crumbs.textContent).toContain("Tareas");
    expect(document.querySelector(".titlebar")!.hasAttribute("data-tauri-drag-region")).toBe(true);
    crumbs.querySelectorAll(".crumb").forEach((c) => expect(c.hasAttribute("data-tauri-drag-region")).toBe(true));
    fireEvent.keyDown(window, { key: "i", ctrlKey: true });
    expect(document.querySelector(".crumbs")!.textContent).toContain("Bandeja");
  });

  it("⌘\\ toggles the rail and remembers it", async () => {
    await mount();
    const before = document.querySelector("aside, .sidebar")!.className;
    fireEvent.keyDown(window, { key: "\\", ctrlKey: true });
    expect(localStorage.getItem("delilah:rail")).toBe("1");
    expect(document.querySelector("aside, .sidebar")!.className).not.toBe(before);
    fireEvent.keyDown(window, { key: "\\", ctrlKey: true });
    expect(localStorage.getItem("delilah:rail")).toBe("0");
  });

  it("greys and disables the content while disconnected, with the banner over main", async () => {
    conn.status = "down";
    await mount();
    expect(screen.getByRole("alert")).toBeTruthy();
    const content = document.querySelector(".shell-content")!;
    expect(content.classList.contains("is-offline")).toBe(true);
    expect(content.getAttribute("aria-disabled")).toBe("true");
    expect(content.hasAttribute("inert")).toBe(true);
    expect(document.querySelector("main")!.contains(screen.getByRole("alert"))).toBe(true);
  });

  it("is not greyed when connected", async () => {
    await mount();
    expect(document.querySelector(".shell-content")!.classList.contains("is-offline")).toBe(false);
    expect(screen.queryByRole("alert")).toBeNull();
  });
});

describe("App delete from the sidebar", () => {
  it("deletes after the sidebar's own confirmation, with no second dialog", async () => {
    mocked.tasks = [
      { key: "PROJ-2", title: "PROJ-2", project: "web", workflow: "feature", phase: null, saving_mode: false, title_source: "initial", previous_title: null, branch_error: null, handoff: null, status: { type: "running" } },
    ];
    mocked.projects = [{ name: "web", path: "/w/web", kind: "repo", workflow: "feature", settings: DEFAULT_PROJECT_SETTINGS }];
    localStorage.setItem("delilah:rail", "0");
    mocked.deleteTask.mockResolvedValue(null);
    await mount();
    fireEvent.contextMenu(document.querySelector(".sb-taskrow")!);
    fireEvent.click(screen.getByRole("menuitem", { name: "Eliminar tarea…" }));
    fireEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Eliminar tarea" }));
    await act(async () => {});
    expect(mocked.deleteTask).toHaveBeenCalledTimes(1);
    expect(mocked.deleteTask).toHaveBeenCalledWith("PROJ-2");
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});
