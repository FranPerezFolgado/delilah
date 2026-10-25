// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { setLanguage } from "../i18n";
import { TaskTabs, hiddenTabs, type TaskTabItem } from "./TaskTabs";

beforeEach(() => setLanguage("es"));
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const tabs: TaskTabItem[] = [
  { id: "task", kind: "task", label: "Tarea" },
  { id: "project", kind: "project", label: "Proyecto" },
  { id: "s1", kind: "session", label: "Claude", dot: "run" },
  { id: "a.rs", kind: "file", label: "a.rs", closable: true },
  { id: "changes", kind: "changes", label: "Cambios", count: 3, closable: true },
  { id: "pr", kind: "pr", label: "PR" },
];
const setup = (active = "task") => {
  const f = { onSelect: vi.fn(), onClose: vi.fn(), onNewSession: vi.fn() };
  render(<TaskTabs tabs={tabs} active={active} {...f} />);
  return f;
};

it("is a tablist with the open tab selected", () => {
  setup("s1");
  const list = screen.getByRole("tablist");
  expect(within(list).getAllByRole("tab")).toHaveLength(6);
  expect(screen.getByRole("tab", { name: /Claude/ }).getAttribute("aria-selected")).toBe("true");
  expect(screen.getByRole("tab", { name: "Tarea" }).getAttribute("aria-selected")).toBe("false");
});

it("selects on click and shows the Cambios count", () => {
  const f = setup();
  fireEvent.click(screen.getByRole("tab", { name: /Cambios/ }));
  expect(f.onSelect).toHaveBeenCalledWith("changes");
  expect(screen.getByRole("tab", { name: /Cambios/ }).textContent).toContain("3");
});

it("only files and Cambios close", () => {
  const f = setup();
  expect(screen.queryByRole("button", { name: "Cerrar PR" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Cerrar a.rs" }));
  expect(f.onClose).toHaveBeenCalledWith("a.rs");
  fireEvent.click(screen.getByRole("button", { name: "Cerrar Cambios" }));
  expect(f.onClose).toHaveBeenCalledWith("changes");
});

it("+ Sesión offers Claude and Terminal with their shortcuts", () => {
  const f = setup();
  fireEvent.click(screen.getByRole("button", { name: "+ Sesión" }));
  const items = screen.getAllByRole("menuitem");
  expect(items.map((i) => i.textContent)).toEqual([expect.stringContaining("Claude"), expect.stringContaining("Terminal")]);
  expect(items[0].textContent).toMatch(/T/);
  fireEvent.click(items[1]);
  expect(f.onNewSession).toHaveBeenCalledWith("terminal");
});

it("hiddenTabs lists the tabs outside the scroll box", () => {
  const box = document.createElement("div");
  Object.defineProperty(box, "clientWidth", { value: 100 });
  const mk = (id: string, left: number) => {
    const el = document.createElement("div");
    el.dataset.tab = id;
    Object.defineProperty(el, "offsetLeft", { value: left });
    Object.defineProperty(el, "offsetWidth", { value: 40 });
    box.appendChild(el);
  };
  mk("a", 0);
  mk("b", 50);
  mk("c", 90);
  expect(hiddenTabs(box)).toEqual(["c"]);
});

it("no layout means no overflow menu", () => {
  setup();
  expect(screen.queryByRole("button", { name: /más/ })).toBeNull();
});
