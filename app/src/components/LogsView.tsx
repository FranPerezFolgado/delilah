import { useEffect, useMemo, useRef, useState } from "react";
import { DAEMON_URL } from "../api";
import { Icon } from "./Icon";

/** Help › Ver logs: what the daemon has been doing, followed live while open. */
export function LogsView({ onClose }: { onClose: () => void }) {
  const [text, setText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState("");
  const [follow, setFollow] = useState(true);
  const box = useRef<HTMLPreElement>(null);

  useEffect(() => {
    let alive = true;
    const load = () =>
      fetch(`${DAEMON_URL}/api/v1/logs?lines=2000`)
        .then((r) => (r.ok ? r.text() : Promise.reject(new Error(`delilahd respondió ${r.status}`))))
        .then((t) => {
          if (!alive) return;
          setText(t);
          setError(null);
        })
        .catch(() => alive && setError("No se pueden leer los logs: ¿está arrancado delilahd?"));
    load();
    const timer = setInterval(load, 2000);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const shown = useMemo(() => {
    const lines = text.split("\n");
    const q = filter.trim().toLowerCase();
    return (q ? lines.filter((l) => l.toLowerCase().includes(q)) : lines).join("\n");
  }, [text, filter]);

  useEffect(() => {
    if (follow && box.current) box.current.scrollTop = box.current.scrollHeight;
  }, [shown, follow]);

  return (
    <div className="backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="dialog logs" role="dialog" aria-label="Logs de delilahd">
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <h2 style={{ flex: 1 }}>Logs de delilahd</h2>
          <input className="input" style={{ width: 220 }} placeholder="Filtrar…" value={filter} onChange={(e) => setFilter(e.target.value)} aria-label="Filtrar los logs" />
          <label className="muted" style={{ display: "flex", gap: 4, alignItems: "center", fontSize: 12 }}>
            <input type="checkbox" checked={follow} onChange={(e) => setFollow(e.target.checked)} /> Seguir
          </label>
          <button className="btn" onClick={() => navigator.clipboard.writeText(shown).catch(() => {})}>Copiar</button>
          <button className="btn btn-ghost" onClick={onClose} aria-label="Cerrar">
            <Icon name="close" />
          </button>
        </div>
        {error && <div className="error">{error}</div>}
        <pre ref={box} className="logs-text selectable">{shown || "Sin logs todavía."}</pre>
        <div className="muted" style={{ fontSize: 11.5 }}>
          Las últimas 2000 líneas del daemon, actualizadas cada 2 s. Los registros de cada sesión están en la carpeta de su tarea
          (<span className="mono">~/.delilah/tasks/&lt;CLAVE&gt;</span>).
        </div>
      </div>
    </div>
  );
}
