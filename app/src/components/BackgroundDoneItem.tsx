import { useState } from "react";
import type { ConversationEntry } from "../api";
import { outcome } from "../backgroundItems";
import { duration, formatTokens } from "../format";
import { t, tn } from "../i18n";
import { Markdown } from "./Markdown";
import "./BackgroundDoneItem.css";

type Props = { entry: Extract<ConversationEntry, { kind: "marker" }>; complete: boolean };

const ICON = { ok: "✓", failed: "⚠", stopped: "■", other: "•" } as const;

/** i18n keys per kind and outcome; `raw` carries an unknown status word. */
const PHRASES = {
  agent: { ok: "Subagente terminado", failed: "Subagente falló", stopped: "Subagente detenido", raw: "Subagente terminado ({status})" },
  command: { ok: "Comando terminado", failed: "Comando falló", stopped: "Comando detenido", raw: "Comando terminado ({status})" },
  task: {
    ok: "Tarea en segundo plano terminada",
    failed: "Tarea en segundo plano falló",
    stopped: "Tarea en segundo plano detenida",
    raw: "Tarea en segundo plano terminada ({status})",
  },
} as const;

/** One row for a finished background task: outcome icon, phrase, label and metrics. */
export function BackgroundDoneItem({ entry, complete }: Props) {
  const [open, setOpen] = useState(false);
  const info = entry.background;
  if (!info) {
    const base = t("Tarea en segundo plano terminada");
    return <div className="conv-marker muted">{entry.detail ? `${base} · ${entry.detail}` : base}</div>;
  }
  const o = outcome(info);
  const s = info.status ?? "";
  const p = PHRASES[info.kind];
  const phrase = o === "other" ? (s ? t(p.raw, { status: s }) : t(p.ok)) : t(p[o]);
  const parts = [phrase];
  if (info.label) parts.push(info.label);
  if (info.duration_ms != null) parts.push(duration(info.duration_ms / 1000));
  if (info.tool_uses != null) parts.push(tn(info.tool_uses, "{n} herramienta", "{n} herramientas"));
  if (info.tokens != null) parts.push(t("{tokens} tokens", { tokens: formatTokens(info.tokens) }));
  if (info.kind === "command" && info.exit_code != null) parts.push(t("código {code}", { code: info.exit_code }));
  const warn = o === "failed" || o === "stopped";
  const expandable = info.kind !== "command" && (!!info.report || !!info.report_in_handback);
  const toggle = (
    <button type="button" className="bg-done-toggle" aria-expanded={open} onClick={() => setOpen(!open)}>
      {open ? "▾" : "▸"}
    </button>
  );
  return (
    <div className={`conv-marker bg-done ${o}${warn ? " warn" : " muted"}${expandable ? " expandable" : ""}`}>
      <div className="bg-done-head">
        <span className="bg-done-icon" aria-hidden="true">{ICON[o]}</span>
        <span className="bg-done-text">{parts.join(" · ")}</span>
        {expandable && !open && toggle}
      </div>
      {expandable && open && (
        <div className="bg-done-report">
          {toggle}
          {info.report ? (
            <>
              <div className="selectable"><Markdown text={info.report} remoteImages="click" /></div>
              {info.report_truncated && <div className="bg-done-note">{t("(informe recortado)")}</div>}
            </>
          ) : (
            <div className="bg-done-note">{complete ? t("Informe no disponible") : t("Cargando informe…")}</div>
          )}
        </div>
      )}
    </div>
  );
}
