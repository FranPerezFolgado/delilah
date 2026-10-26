import { useState } from "react";
import type { ConversationEntry } from "../api";
import { outcome } from "../backgroundItems";
import { duration, formatTokens } from "../format";
import { t, tn } from "../i18n";
import { Markdown } from "./Markdown";
import "./BackgroundDoneItem.css";

type Props = { entry: Extract<ConversationEntry, { kind: "marker" }>; complete: boolean };

const ICON = { ok: "✓", failed: "⚠", stopped: "■", other: "•" } as const;

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
  let phrase: string;
  if (info.kind === "agent") {
    phrase = o === "ok" ? t("Subagente terminado") : o === "failed" ? t("Subagente falló") : o === "stopped" ? t("Subagente detenido") : s ? t("Subagente terminado ({status})", { status: s }) : t("Subagente terminado");
  } else if (info.kind === "command") {
    phrase = o === "ok" ? t("Comando terminado") : o === "failed" ? t("Comando falló") : o === "stopped" ? t("Comando detenido") : s ? t("Comando terminado ({status})", { status: s }) : t("Comando terminado");
  } else {
    phrase = o === "ok" ? t("Tarea en segundo plano terminada") : o === "failed" ? t("Tarea en segundo plano falló") : o === "stopped" ? t("Tarea en segundo plano detenida") : s ? t("Tarea en segundo plano terminada ({status})", { status: s }) : t("Tarea en segundo plano terminada");
  }
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
