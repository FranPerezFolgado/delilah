// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Menu } from "./Menu";

afterEach(cleanup);

function setup(onSelect = vi.fn()) {
  render(
    <Menu
      trigger={(p) => <button {...p}>más</button>}
      items={[
        { label: "Uno", onSelect },
        { label: "Dos", onSelect: () => {}, disabled: true },
        { label: "Tres", onSelect: () => {}, danger: true, shortcut: "⌘T" },
      ]}
    />,
  );
  const trigger = screen.getByText("más");
  trigger.focus();
  fireEvent.click(trigger);
  return { trigger, menu: screen.getByRole("menu") };
}

describe("Menu", () => {
  it("is closed until the trigger is clicked, then lists menuitems", () => {
    render(<Menu trigger={(p) => <button {...p}>m</button>} items={[{ label: "A", onSelect: () => {} }]} />);
    expect(screen.queryByRole("menu")).toBeNull();
    fireEvent.click(screen.getByText("m"));
    expect(screen.getAllByRole("menuitem").length).toBe(1);
    expect(screen.getByText("m").getAttribute("aria-expanded")).toBe("true");
  });

  it("focuses the first item and moves with arrows, skipping disabled, wrapping", () => {
    const { menu } = setup();
    const items = screen.getAllByRole("menuitem");
    expect(document.activeElement).toBe(items[0]);
    fireEvent.keyDown(menu, { key: "ArrowDown" });
    expect(document.activeElement).toBe(items[2]);
    fireEvent.keyDown(menu, { key: "ArrowDown" });
    expect(document.activeElement).toBe(items[0]);
    fireEvent.keyDown(menu, { key: "ArrowUp" });
    expect(document.activeElement).toBe(items[2]);
  });

  it("Home and End jump to the ends", () => {
    const { menu } = setup();
    const items = screen.getAllByRole("menuitem");
    fireEvent.keyDown(menu, { key: "End" });
    expect(document.activeElement).toBe(items[2]);
    fireEvent.keyDown(menu, { key: "Home" });
    expect(document.activeElement).toBe(items[0]);
  });

  it("Escape closes and returns focus to the trigger", () => {
    const { trigger, menu } = setup();
    fireEvent.keyDown(menu, { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();
    expect(document.activeElement).toBe(trigger);
  });

  it("selecting runs the handler, closes and returns focus", () => {
    const onSelect = vi.fn();
    const { trigger } = setup(onSelect);
    fireEvent.click(screen.getByText("Uno"));
    expect(onSelect).toHaveBeenCalled();
    expect(screen.queryByRole("menu")).toBeNull();
    expect(document.activeElement).toBe(trigger);
  });

  it("disabled items do not fire; danger and shortcut render", () => {
    const { menu } = setup();
    const dos = screen.getByText("Dos").closest("button")!;
    expect(dos.disabled).toBe(true);
    expect(screen.getByText("Tres").closest("button")!.className).toContain("danger");
    expect(menu.textContent).toContain("⌘T");
  });

  it("closes on an outside press", () => {
    setup();
    fireEvent.mouseDown(document.body);
    expect(screen.queryByRole("menu")).toBeNull();
  });
});
