import { useEffect, useRef } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import "@xterm/xterm/css/xterm.css";
import { terminalUrl } from "../api";

const THEME = {
  background: "#0c0d0f",
  foreground: "#e7e8ea",
  cursor: "#f0a03c",
  selectionBackground: "rgba(240,160,60,0.3)",
};

/** The agent's live terminal: the daemon's WebSocket stream drawn by xterm.js. */
export function TerminalPane({ taskKey, focused }: { taskKey: string; focused: boolean }) {
  const host = useRef<HTMLDivElement>(null);
  const term = useRef<Terminal | null>(null);

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
    term.current = t;

    const ws = new WebSocket(terminalUrl(taskKey));
    ws.binaryType = "arraybuffer";
    const sendSize = () => {
      if (ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify({ type: "resize", cols: t.cols, rows: t.rows }));
    };
    ws.onopen = sendSize;
    ws.onmessage = (e) => {
      if (e.data instanceof ArrayBuffer) t.write(new Uint8Array(e.data));
    };
    ws.onclose = () => t.write("\r\n\x1b[2m[sin agente en marcha: la sesión terminó o no ha empezado]\x1b[0m\r\n");
    const encoder = new TextEncoder();
    const input = t.onData((data) => {
      if (ws.readyState === WebSocket.OPEN) ws.send(encoder.encode(data));
    });

    const fitNow = () => {
      try {
        fit.fit();
        sendSize();
      } catch {
        // Not laid out yet.
      }
    };
    const observer = new ResizeObserver(fitNow);
    observer.observe(el);
    fitNow();

    return () => {
      observer.disconnect();
      input.dispose();
      ws.close();
      t.dispose();
      term.current = null;
    };
  }, [taskKey]);

  useEffect(() => {
    if (focused) term.current?.focus();
  }, [focused]);

  return <div className="pane-term" ref={host} />;
}
