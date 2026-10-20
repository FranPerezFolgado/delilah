import { useEffect, useRef, type KeyboardEvent as ReactKeyboardEvent, type PointerEvent as ReactPointerEvent } from "react";
import { clampTreeWidth, setTreeWidth, TREE_WIDTH } from "../editor/session";
import { t } from "../i18n";

/**
 * The tree's left edge: drag it to widen or narrow the tree, double-click to go back
 * to the default width. The main column always keeps some room. `controls` is the
 * tree panel's id.
 */
export function TreeResizer({ width, controls, onChange }: { width: number; controls: string; onChange: (width: number) => void }) {
  // Ends a drag in progress: also on unmount, e.g. the tree hidden mid-drag.
  const endDrag = useRef<(() => void) | null>(null);
  useEffect(() => () => endDrag.current?.(), []);

  const commit = (w: number) => {
    if (w === width) return;
    onChange(w);
    setTreeWidth(w);
  };
  const screenOf = (el: HTMLElement) => el.closest<HTMLElement>(".tscreen");
  const clampIn = (w: number, screen: HTMLElement | null) => clampTreeWidth(w, screen?.clientWidth);

  const onPointerDown = (e: ReactPointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.preventDefault();
    const handle = e.currentTarget;
    const screen = screenOf(handle);
    const startX = e.clientX;
    let last = width;
    handle.setPointerCapture(e.pointerId);
    document.body.classList.add("resizing");
    // While dragging, only the CSS variable moves: no re-render on every pointer move.
    const move = (ev: PointerEvent) => {
      // The tree is on the right: dragging left makes it wider.
      last = clampIn(width + startX - ev.clientX, screen);
      screen?.style.setProperty("--tree-w", `${last}px`);
    };
    // Lost capture comes after pointerup and pointercancel alike.
    const end = () => {
      endDrag.current = null;
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("lostpointercapture", end);
      document.body.classList.remove("resizing");
      commit(last);
    };
    endDrag.current = end;
    handle.addEventListener("pointermove", move);
    handle.addEventListener("lostpointercapture", end);
  };

  const onKeyDown = (e: ReactKeyboardEvent<HTMLDivElement>) => {
    const screen = screenOf(e.currentTarget);
    const step = e.shiftKey ? 48 : 16;
    const next: Record<string, number> = {
      ArrowLeft: width + step,
      ArrowRight: width - step,
      Home: TREE_WIDTH.min,
      End: TREE_WIDTH.max,
    };
    if (!(e.key in next)) return;
    e.preventDefault();
    commit(clampIn(next[e.key], screen));
  };

  return (
    <div
      className="tresize"
      role="separator"
      aria-orientation="vertical"
      aria-controls={controls}
      aria-label={t("Cambiar el ancho del árbol")}
      aria-valuemin={TREE_WIDTH.min}
      aria-valuemax={TREE_WIDTH.max}
      aria-valuenow={width}
      tabIndex={0}
      title={t("Arrastra para cambiar el ancho; doble clic para restablecerlo")}
      onPointerDown={onPointerDown}
      onDoubleClick={(e) => commit(clampIn(TREE_WIDTH.default, screenOf(e.currentTarget)))}
      onKeyDown={onKeyDown}
    />
  );
}
