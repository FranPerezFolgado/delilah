import { useEffect, useMemo, useRef, useState } from "react";
import { DAEMON_URL } from "../api";
import { Button, Dialog, Segmented } from "./ui";
import { parseLogs, passes, type LogFilter } from "../logLevel";
import { t } from "../i18n";
import "./LogsView.css";

/** Daemon logs, followed live while open. Reached from the statusbar and Ajustes (`delilah:show-logs`). */
export function LogsView({ onClose }: { onClose: () => void }) {
  const [text, setText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [level, setLevel] = useState<LogFilter>("all");
  const [follow, setFollow] = useState(true);
  const box = useRef<HTMLPreElement>(null);

  useEffect(() => {
    let alive = true;
    const load = () =>
      fetch(`${DAEMON_URL}/api/v1/logs?lines=2000`)
        .then((r) => (r.ok ? r.text() : Promise.reject(new Error(t("delilahd respondió {status}", { status: r.status })))))
        .then((body) => {
          if (!alive) return;
          setText(body);
          setError(null);
        })
        .catch(() => alive && setError(t("No se pueden leer los logs: ¿está arrancado delilahd?")));
    load();
    const timer = setInterval(load, 2000);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, []);

  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    return parseLogs(text).filter((l) => passes(l.level, level) && (!q || l.line.toLowerCase().includes(q)));
  }, [text, query, level]);

  useEffect(() => {
    if (follow && box.current) box.current.scrollTop = box.current.scrollHeight;
  }, [shown, follow]);

  const copy = () => navigator.clipboard.writeText(shown.map((l) => l.line).join("\n")).catch(() => {});

  return (
    <Dialog
      title={t("Logs de delilahd")}
      onClose={onClose}
      size={{ w: 1000, h: 640 }}
      footer={<Button variant="secondary" onClick={onClose}>{t("Cerrar")}</Button>}
    >
      <div className="logs-body">
        <div className="logs-bar">
          <Segmented
            mode="radio"
            label={t("Nivel")}
            value={level}
            onChange={(v) => setLevel(v as LogFilter)}
            options={[
              { value: "all", label: t("Todo") },
              { value: "info", label: t("Info") },
              { value: "warn", label: t("Advertencias") },
              { value: "error", label: t("Errores") },
            ]}
          />
          <input className="input logs-q" placeholder={t("Filtrar…")} value={query} onChange={(e) => setQuery(e.target.value)} aria-label={t("Filtrar los logs")} />
          <label className="logs-follow">
            <input type="checkbox" checked={follow} onChange={(e) => setFollow(e.target.checked)} /> {t("Seguir")}
          </label>
          <Button variant="secondary" size={28} onClick={copy}>{t("Copiar")}</Button>
        </div>
        {error && <div className="logs-err" role="alert">{error}</div>}
        <pre ref={box} className="logs-text selectable">
          {shown.length === 0 || (shown.length === 1 && !shown[0].line)
            ? t("Sin logs todavía.")
            : shown.map((l, i) => (
                <span key={i} className={`ll ll-${l.level ?? "none"}`}>{l.line + "\n"}</span>
              ))}
        </pre>
        <div className="logs-note">
          {t("Las últimas 2000 líneas del daemon, actualizadas cada 2 s. Los registros de cada sesión están en la carpeta de su tarea (~/.delilah/tasks/<CLAVE>).")}
        </div>
      </div>
    </Dialog>
  );
}
