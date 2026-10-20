import { useEffect, useRef } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import "@xterm/xterm/css/xterm.css";
import { terminalUrl } from "../api";
import { actionFor } from "../shortcuts";
import { t as tr } from "../i18n";

const THEME = {
  background: "#0c0d0f",
  foreground: "#e7e8ea",
  cursor: "#f0a03c",
  selectionBackground: "rgba(240,160,60,0.3)",
};

/**
 * A session's live terminal: the daemon's WebSocket stream drawn by xterm.js.
 *
 * A PTY has one size, and several windows may show it (the app, a browser, the
 * phone). If each imposed its own, the agent's TUI would erase the wrong number of
 * lines and leave leftovers on screen. So, as in tmux, the focused pane drives the
 * size and every other viewer draws at the size the daemon announces. Only the
 * window in use drives: a pane focused in a window in the background does not.
 */
export function TerminalPane({ sessionId, focused }: { sessionId: string; focused: boolean }) {
  const host = useRef<HTMLDivElement>(null);
  const focusedRef = useRef(focused);
  const claimRef = useRef<() => void>(() => {});
  const termRef = useRef<Terminal | null>(null);

  useEffect(() => {
    const el = host.current;
    if (!el) return;
    const t = new Terminal({
      fontFamily: 'ui-monospace, "SF Mono", Menlo, monospace',
      fontSize: 12,
      lineHeight: 1.15,
      cursorBlink: true,
      theme: THEME,
      scrollback: 5000,
    });
    const fit = new FitAddon();
    t.loadAddon(fit);
    t.open(el);
    try {
      t.loadAddon(new WebglAddon());
    } catch {
      // No WebGL: xterm falls back to its DOM renderer.
    }
    termRef.current = t;
    // App shortcuts (⌘T, ⌘W…) are for the app, not the shell.
    t.attachCustomKeyEventHandler((e) => !(e.type === "keydown" && actionFor(e)));

    const ws = new WebSocket(terminalUrl(sessionId));
    ws.binaryType = "arraybuffer";

    // Take over the size: fit this pane and tell the daemon, which tells the others.
    const claim = () => {
      const dims = fit.proposeDimensions();
      if (!dims || !Number.isFinite(dims.cols) || !Number.isFinite(dims.rows)) return;
      const cols = Math.max(20, dims.cols);
      const rows = Math.max(5, dims.rows);
      if (t.cols !== cols || t.rows !== rows) t.resize(cols, rows);
      if (ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify({ type: "resize", cols, rows }));
    };
    claimRef.current = claim;

    const driving = () => focusedRef.current && document.hasFocus();
    ws.onopen = () => {
      if (driving()) claim();
    };
    // Coming back to this window: its focused pane takes the size back.
    const onWindowFocus = () => {
      if (focusedRef.current) claim();
    };
    window.addEventListener("focus", onWindowFocus);
    ws.onmessage = (e) => {
      if (e.data instanceof ArrayBuffer) {
        t.write(new Uint8Array(e.data));
        return;
      }
      try {
        const msg = JSON.parse(e.data as string);
        // Someone else is driving: draw at their size.
        if (msg.type === "size" && (t.cols !== msg.cols || t.rows !== msg.rows) && !driving()) {
          t.resize(msg.cols, msg.rows);
        }
      } catch {
        // Not a control message.
      }
    };
    ws.onclose = () => t.write(`\r\n\x1b[2m[${tr("la sesión no está en marcha")}]\x1b[0m\r\n`);

    const encoder = new TextEncoder();
    const input = t.onData((data) => {
      if (ws.readyState !== WebSocket.OPEN) return;
      // Typing here makes this pane the driver.
      claim();
      ws.send(encoder.encode(data));
    });

    const observer = new ResizeObserver(() => {
      if (driving()) claim();
    });
    observer.observe(el);

    return () => {
      window.removeEventListener("focus", onWindowFocus);
      observer.disconnect();
      input.dispose();
      ws.close();
      t.dispose();
      termRef.current = null;
    };
  }, [sessionId]);

  useEffect(() => {
    focusedRef.current = focused;
    if (focused && document.hasFocus()) {
      claimRef.current();
      termRef.current?.focus();
    }
  }, [focused]);

  return <div className="pane-term" ref={host} />;
}
