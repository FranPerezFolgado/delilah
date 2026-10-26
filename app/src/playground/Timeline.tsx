import { useEffect, useRef, type KeyboardEvent } from "react";
import type { ActionKind } from "../api";
import { t } from "../i18n";
import { KIND_COLOURS } from "./art/kinds";
import type { TimelineRow } from "./flowModel";
import "./Timeline.css";

interface Props {
  rows: TimelineRow[];
  selected: string | null;
  onSelect(node: string): void;
  className?: string;
}

const kindName = (k: ActionKind | undefined): string => {
  switch (k) {
    case "edit": return t("Edición");
    case "read": return t("Lectura");
    case "run": return t("Comando");
    case "delegate": return t("Delegación");
    default: return t("Otra");
  }
};

const eventName = (r: TimelineRow): string => {
  switch (r.event) {
    case "tool_started": return t("empieza");
    case "tool_ended": return r.failed ? t("falla") : t("termina");
    case "helper_started": return t("ayudante entra");
    case "helper_ended": return t("ayudante sale");
    case "turn_over": return t("fin del turno");
  }
};

const pad = (n: number) => String(n).padStart(2, "0");
export function formatTime(at: string): string {
  const d = new Date(at);
  if (Number.isNaN(d.getTime())) return "";
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

export function Timeline({ rows, selected, onSelect, className }: Props) {
  const refs = useRef(new Map<number, HTMLButtonElement>());
  const selRow = rows.findIndex((r) => r.node === selected);

  useEffect(() => {
    if (selRow < 0) return;
    refs.current.get(selRow)?.scrollIntoView?.({ block: "nearest" });
  }, [selRow]);

  if (rows.length === 0) return null;

  const move = (to: number) => {
    const i = Math.max(0, Math.min(rows.length - 1, to));
    refs.current.get(i)?.focus();
    onSelect(rows[i].node);
  };

  const onKey = (e: KeyboardEvent, i: number) => {
    if (e.key === "ArrowDown") move(i + 1);
    else if (e.key === "ArrowUp") move(i - 1);
    else if (e.key === "Home") move(0);
    else if (e.key === "End") move(rows.length - 1);
    else return;
    e.preventDefault();
  };

  const tabStop = selRow >= 0 ? selRow : 0;

  return (
    <div role="listbox" aria-label={t("Línea de tiempo")} className={`timeline${className ? ` ${className}` : ""}`}>
      {rows.map((r, i) => {
        const time = formatTime(r.at);
        const kind = kindName(r.kind);
        const what = [r.tool, r.target].filter(Boolean).join(" ");
        const label = [time, kind, what, eventName(r), r.failed ? t("fallida") : ""].filter(Boolean).join(", ");
        return (
          <button
            key={r.seq}
            ref={(el) => {
              if (el) refs.current.set(i, el);
              else refs.current.delete(i);
            }}
            type="button"
            role="option"
            aria-selected={i === selRow}
            aria-label={label}
            tabIndex={i === tabStop ? 0 : -1}
            className={`timeline-row${r.failed ? " failed" : ""}`}
            onClick={() => onSelect(r.node)}
            onKeyDown={(e) => onKey(e, i)}
          >
            <span className="timeline-time">{time}</span>
            <span className="timeline-kind">
              <span className="timeline-swatch" style={{ background: r.kind ? KIND_COLOURS[r.kind] : KIND_COLOURS.other }} />
              {kind}
            </span>
            <span className="timeline-detail">
              {r.tool}
              {r.target ? <span className="timeline-target"> {r.target}</span> : null}
              {" · "}
              {eventName(r)}
              {r.failed ? <span className="timeline-fail"> ✕</span> : null}
            </span>
          </button>
        );
      })}
    </div>
  );
}
