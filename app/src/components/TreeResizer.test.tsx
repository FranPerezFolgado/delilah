// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render } from "@testing-library/react";
import { TREE_WIDTH, treeWidth } from "../editor/session";
import { TreeResizer } from "./TreeResizer";

/** The resizer inside a `.tscreen` of `screenWidth` pixels (jsdom lays nothing out). */
function setup(width: number, screenWidth = 1200) {
  const onChange = vi.fn();
  const { container, unmount } = render(
    <div className="tscreen">
      <TreeResizer width={width} controls="tree" onChange={onChange} />
    </div>,
  );
  const screen = container.querySelector<HTMLElement>(".tscreen")!;
  Object.defineProperty(screen, "clientWidth", { value: screenWidth });
  const handle = container.querySelector<HTMLElement>(".tresize")!;
  // jsdom has no pointer capture: enough to let a drag start.
  handle.setPointerCapture = vi.fn();
  return { onChange, screen, handle, unmount };
}

afterEach(() => {
  cleanup();
  localStorage.clear();
  document.body.classList.remove("resizing");
});

describe("TreeResizer from the keyboard", () => {
  it.each([
    ["ArrowLeft", false, 276],
    ["ArrowLeft", true, 308],
    ["ArrowRight", false, 244],
    ["Home", false, TREE_WIDTH.min],
    ["End", false, TREE_WIDTH.max],
  ])("%s (shift: %s) sets %i and remembers it", (key, shiftKey, expected) => {
    const { onChange, handle } = setup(260);
    fireEvent.keyDown(handle, { key, shiftKey });
    expect(onChange).toHaveBeenCalledWith(expected);
    expect(treeWidth()).toBe(expected);
  });

  it("stops at the room the main column needs", () => {
    const { onChange, handle } = setup(260, 800);
    fireEvent.keyDown(handle, { key: "End" });
    expect(onChange).toHaveBeenCalledWith(800 - TREE_WIDTH.mainMin);
  });

  it("changes nothing at a limit already reached", () => {
    const { onChange, handle } = setup(TREE_WIDTH.min);
    fireEvent.keyDown(handle, { key: "ArrowRight" });
    expect(onChange).not.toHaveBeenCalled();
    expect(localStorage.length).toBe(0);
  });

  it("leaves other keys alone", () => {
    const { onChange, handle } = setup(260);
    fireEvent.keyDown(handle, { key: "a" });
    expect(onChange).not.toHaveBeenCalled();
  });
});

describe("TreeResizer with the pointer", () => {
  it("goes back to the default width on double-click", () => {
    const { onChange, handle } = setup(400);
    fireEvent.doubleClick(handle);
    expect(onChange).toHaveBeenCalledWith(TREE_WIDTH.default);
  });

  it("moves only the CSS variable while dragging and commits once at the end", () => {
    const { onChange, screen, handle } = setup(260);
    fireEvent.pointerDown(handle, { button: 0, clientX: 500, pointerId: 1 });
    expect(document.body.classList.contains("resizing")).toBe(true);
    fireEvent.pointerMove(handle, { clientX: 470 });
    fireEvent.pointerMove(handle, { clientX: 450 });
    expect(screen.style.getPropertyValue("--tree-w")).toBe("310px");
    expect(onChange).not.toHaveBeenCalled();

    fireEvent(handle, new Event("lostpointercapture"));
    expect(onChange).toHaveBeenCalledOnce();
    expect(onChange).toHaveBeenCalledWith(310);
    expect(treeWidth()).toBe(310);
    expect(document.body.classList.contains("resizing")).toBe(false);
  });

  it("does not save a click that moved nothing", () => {
    const { onChange, handle } = setup(260);
    fireEvent.pointerDown(handle, { button: 0, clientX: 500, pointerId: 1 });
    fireEvent(handle, new Event("lostpointercapture"));
    expect(onChange).not.toHaveBeenCalled();
    expect(localStorage.length).toBe(0);
  });

  it("ignores buttons other than the main one", () => {
    const { handle } = setup(260);
    fireEvent.pointerDown(handle, { button: 2, clientX: 500, pointerId: 1 });
    expect(document.body.classList.contains("resizing")).toBe(false);
  });

  it("ends a drag cut short by the tree going away", () => {
    const { onChange, handle, unmount } = setup(260);
    fireEvent.pointerDown(handle, { button: 0, clientX: 500, pointerId: 1 });
    fireEvent.pointerMove(handle, { clientX: 400 });
    unmount();
    expect(document.body.classList.contains("resizing")).toBe(false);
    expect(onChange).toHaveBeenCalledWith(360);
  });
});
