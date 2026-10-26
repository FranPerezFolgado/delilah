import { useEffect, useRef } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import "@xterm/xterm/css/xterm.css";
import { terminalUrl, type SessionKind } from "../api";
import { actionFor } from "../shortcuts";
import { AGENT_NEW_LINE, isAgentNewLineKey } from "../keys";
import { t as tr } from "../i18n";
import { cssVar, onSchemeChange } from "../theme";
import "./TerminalPane.css";

/** xterm's colours from the app's tokens, read when the scheme changes. */
const xtermTheme = () => ({
  background: cssVar("--term-bg"),
  foreground: cssVar("--term-tx"),
  cursor: cssVar("--tx-strong"),
  selectionBackground: cssVar("--sel"),
});
// How long a pane's size must hold still before it claims it.
const RESIZE_SETTLE_MS = 100;

/**
 * A session's live terminal: the daemon's WebSocket stream drawn by xterm.js.
 *
 * A PTY has one size, and several windows may show it (the app, a browser, the
 * phone). If each imposed its own, the agent's TUI would erase the wrong number of
 * lines and leave leftovers on screen. So, as in tmux, the focused pane drives the
 * size and every other viewer draws at the size the daemon announces. Only the
 * window in use drives: a pane focused in a window in the background does not.
 */
export function TerminalPane({
  sessionId,
  kind,
  focused,
}: {
  sessionId: string;
  kind: SessionKind;
  focused: boolean;
}) {
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
      theme: xtermTheme(),
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
    const offScheme = onSchemeChange(() => {
      t.options.theme = xtermTheme();
    });

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
    t.attachCustomKeyEventHandler((e) => {
      // Shift+Enter is a new line for the agent, not a send.
      if (isAgentNewLineKey(e, kind === "claude")) {
        if (e.type === "keydown" && ws.readyState === WebSocket.OPEN) {
          claim();
          ws.send(encoder.encode(AGENT_NEW_LINE));
        }
        return false;
      }
      // App shortcuts (⌘T, ⌘W…) are for the app, not the shell.
      return !(e.type === "keydown" && actionFor(e));
    });
    const input = t.onData((data) => {
      if (ws.readyState !== WebSocket.OPEN) return;
      // Typing here makes this pane the driver.
      claim();
      ws.send(encoder.encode(data));
    });

    // Once the size settles: dragging the tree's edge or the window's would otherwise
    // send the PTY a resize, and the TUI a redraw, on every frame.
    let settle: ReturnType<typeof setTimeout> | undefined;
    const observer = new ResizeObserver(() => {
      clearTimeout(settle);
      settle = setTimeout(() => {
        if (driving()) claim();
      }, RESIZE_SETTLE_MS);
    });
    observer.observe(el);

    return () => {
      window.removeEventListener("focus", onWindowFocus);
      clearTimeout(settle);
      observer.disconnect();
      input.dispose();
      ws.close();
      offScheme();
      t.dispose();
      termRef.current = null;
    };
  }, [sessionId, kind]);

  useEffect(() => {
    focusedRef.current = focused;
    if (focused && document.hasFocus()) {
      claimRef.current();
      termRef.current?.focus();
    }
  }, [focused]);

  return <div className="pane-term" ref={host} />;
}
