import { useEffect, useMemo, useRef, useState, type KeyboardEvent, type PointerEvent, type ReactNode } from "react";
import type { ActionKind } from "../api";
import { t } from "../i18n";
import { KIND_COLOURS } from "./art/kinds";
import { buildFlow, type FlowNode } from "./flowModel";
import { layoutFlow, type Box } from "./flowLayout";
import { Timeline, formatTime } from "./Timeline";
import { useActivityHistory } from "./useActivityHistory";
import "./FlowView.css";

export interface FlowViewProps {
  /** A task key or `@project`. */
  owner: string;
  online?: boolean;
  /** Goes at the right of the header (the Oficina/Flujo toggle). */
  headerExtra?: ReactNode;
  className?: string;
}

const MARGIN = 24;
const MIN_W = 40;
const MAX_W = 20000;

const kindName = (k: ActionKind | undefined): string => {
  switch (k) {
    case "edit": return t("Edición");
    case "read": return t("Lectura");
    case "run": return t("Comando");
    case "delegate": return t("Delegación");
    default: return t("Otra");
  }
};

const stateName = (n: FlowNode): string => {
  switch (n.state) {
    case "running": return t("en curso");
    case "done": return t("terminada");
    case "failed": return t("fallida");
    default: return t("desconocida");
  }
};

const fit = (b: Box): Box => ({ x: b.x - MARGIN, y: b.y - MARGIN, w: Math.max(b.w, 1) + 2 * MARGIN, h: Math.max(b.h, 1) + 2 * MARGIN });

function useHidden(): boolean {
  const [hidden, setHidden] = useState(() => document.hidden);
  useEffect(() => {
    const on = () => setHidden(document.hidden);
    on();
    document.addEventListener("visibilitychange", on);
    return () => document.removeEventListener("visibilitychange", on);
  }, []);
  return hidden;
}

const clip = (s: string, n: number) => (s.length > n ? `${s.slice(0, n - 1)}…` : s);

export function duration(n: FlowNode): string {
  if (!n.end) return t("en curso");
  const ms = Date.parse(n.end) - Date.parse(n.start);
  if (Number.isNaN(ms) || ms < 0) return "";
  if (ms < 1000) return `${ms} ms`;
  const s = ms / 1000;
  if (s < 60) return `${s.toFixed(1)} s`;
  return `${Math.floor(s / 60)} min ${Math.round(s % 60)} s`;
}

/** The Flujo view of an owner: its agents, helpers and tool calls as a pannable, zoomable graph. */
export function FlowView({ owner, online, headerExtra, className }: FlowViewProps) {
  const { startedAt, sessions, stale } = useActivityHistory(owner, online);
  const hidden = useHidden();
  const model = useMemo(() => buildFlow({ started_at: startedAt ?? "", sessions }), [startedAt, sessions]);
  const layout = useMemo(() => layoutFlow(model), [model]);
  const [selected, setSelected] = useState<string | null>(null);
  const [view, setView] = useState<Box | null>(null);
  const touched = useRef(false);
  const svgRef = useRef<SVGSVGElement>(null);
  const drag = useRef<{ x: number; y: number; vb: Box } | null>(null);

  // Until the user pans or zooms, the view follows the graph as it grows.
  const fitted = useMemo(() => fit(layout.bounds), [layout.bounds]);
  const vb = touched.current && view ? view : fitted;
  const vbRef = useRef(vb);
  vbRef.current = vb;

  const change = (next: Box) => {
    touched.current = true;
    setView(next);
  };
  const zoom = (factor: number, cx?: number, cy?: number) => {
    const v = vbRef.current;
    const w = Math.min(MAX_W, Math.max(MIN_W, v.w * factor));
    const k = w / v.w;
    const px = cx ?? v.x + v.w / 2;
    const py = cy ?? v.y + v.h / 2;
    change({ x: px - (px - v.x) * k, y: py - (py - v.y) * k, w, h: v.h * k });
  };
  const reset = () => {
    touched.current = false;
    setView(null);
  };

  // viewBox units per screen pixel, with the content centred (preserveAspectRatio meet).
  const metrics = () => {
    const el = svgRef.current;
    const r = el?.getBoundingClientRect();
    const v = vbRef.current;
    if (!r || r.width === 0 || r.height === 0) return { s: 1, ox: 0, oy: 0, left: r?.left ?? 0, top: r?.top ?? 0 };
    const s = Math.max(v.w / r.width, v.h / r.height);
    return { s, ox: (r.width - v.w / s) / 2, oy: (r.height - v.h / s) / 2, left: r.left, top: r.top };
  };

  useEffect(() => {
    const el = svgRef.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const m = metrics();
      const v = vbRef.current;
      zoom(e.deltaY < 0 ? 0.85 : 1 / 0.85, v.x + (e.clientX - m.left - m.ox) * m.s, v.y + (e.clientY - m.top - m.oy) * m.s);
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  });

  const onPointerDown = (e: PointerEvent<SVGSVGElement>) => {
    if (e.button !== 0) return;
    drag.current = { x: e.clientX, y: e.clientY, vb: vbRef.current };
    try { e.currentTarget.setPointerCapture?.(e.pointerId); } catch { /* not capturable */ }
  };
  const onPointerMove = (e: PointerEvent<SVGSVGElement>) => {
    const d = drag.current;
    if (!d) return;
    const { s } = metrics();
    change({ ...d.vb, x: d.vb.x - (e.clientX - d.x) * s, y: d.vb.y - (e.clientY - d.y) * s });
  };
  const onPointerUp = () => { drag.current = null; };

  const select = (id: string) => setSelected(id);
  const onNodeKey = (e: KeyboardEvent, id: string) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      select(id);
    }
  };

  const node = model.nodes.find((n) => n.id === selected) ?? null;
  const cls = ["fv", stale && "fv--stale", hidden && "pg-paused", className].filter(Boolean).join(" ");
  const placed = model.nodes.filter((n) => layout.positions[n.id]);

  return (
    <section className={cls} aria-label={t("Flujo de {owner}", { owner })}>
      <header className="fv-head">
        <span className="fv-title"><b>{owner}</b></span>
        <span className="fv-spacer" />
        <span className="fv-zoom">
          <button type="button" aria-label={t("Acercar")} onClick={() => zoom(0.8)}>+</button>
          <button type="button" aria-label={t("Alejar")} onClick={() => zoom(1.25)}>−</button>
          <button type="button" onClick={reset}>{t("Ajustar")}</button>
        </span>
        {headerExtra}
      </header>
      {stale && <div className="fv-banner" role="status">{t("Reconectando…")}</div>}
      {model.notes.dropped && <p className="fv-note">{t("Se perdió parte del historial; el gráfico puede estar incompleto")}</p>}
      {model.notes.empty ? (
        <p className="fv-note">{t("Sin actividad registrada todavía")}</p>
      ) : (
        <div className="fv-body">
          <svg
            ref={svgRef}
            className="fv-svg"
            viewBox={`${vb.x} ${vb.y} ${vb.w} ${vb.h}`}
            role="group"
            aria-label={t("Gráfico de actividad")}
            onPointerDown={onPointerDown}
            onPointerMove={onPointerMove}
            onPointerUp={onPointerUp}
            onPointerCancel={onPointerUp}
          >
            {model.edges.map((e) => {
              const a = layout.positions[e.from];
              const b = layout.positions[e.to];
              if (!a || !b) return null;
              const d = a.x === b.x
                ? `M${a.x + 10} ${a.y + a.h} V${b.y + b.h / 2} H${b.x}`
                : `M${a.x + a.w} ${a.y + a.h / 2} H${(a.x + a.w + b.x) / 2} V${b.y + b.h / 2} H${b.x}`;
              return <path key={`${e.from}>${e.to}`} className="fv-edge" d={d} />;
            })}
            {placed.map((n) => {
              const p = layout.positions[n.id];
              const colour = n.kind ? KIND_COLOURS[n.kind] : KIND_COLOURS.other;
              const what = n.type === "tool" ? [kindName(n.kind), n.target].filter(Boolean).join(" · ") : n.type === "agent" ? t("Agente") : t("Ayudante");
              const label = [n.label, what, stateName(n)].join(", ");
              return (
                <g
                  key={n.id}
                  role="button"
                  tabIndex={0}
                  aria-label={label}
                  aria-pressed={n.id === selected}
                  data-state={n.state}
                  className={`fv-node fv-node-${n.state === "done" ? "ok" : n.state}${n.id === selected ? " fv-node-sel" : ""}`}
                  onClick={() => select(n.id)}
                  onKeyDown={(e) => onNodeKey(e, n.id)}
                >
                  <rect x={p.x} y={p.y} width={p.w} height={p.h} rx={7} className="fv-box" />
                  {n.type === "tool" && <rect x={p.x + 8} y={p.y + 8} width={8} height={8} fill={colour} />}
                  <text x={p.x + (n.type === "tool" ? 22 : 8)} y={p.y + 15} className="fv-label">{clip(n.label, 22)}</text>
                  <text x={p.x + 8} y={p.y + 29} className="fv-sub">{clip(what, 28)}</text>
                </g>
              );
            })}
          </svg>
          <aside className="fv-side">
            <div className="fv-details" aria-live="polite">
              {node ? (
                <dl>
                  <dt>{t("Herramienta")}</dt><dd>{node.tool ?? node.label}</dd>
                  {node.target ? <><dt>{t("Objetivo")}</dt><dd>{node.target}</dd></> : null}
                  <dt>{t("Inicio")}</dt><dd>{formatTime(node.start)}</dd>
                  <dt>{t("Duración")}</dt><dd>{duration(node)}</dd>
                </dl>
              ) : (
                <p>{t("Elige un nodo para ver sus detalles")}</p>
              )}
            </div>
            <Timeline rows={model.timeline} selected={selected} onSelect={select} className="fv-timeline" />
          </aside>
        </div>
      )}
    </section>
  );
}
