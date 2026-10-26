import { useEffect, useId, useRef, useState, type CSSProperties } from "react";
import type { ActionKind, Activity, Session } from "../api";
import { t } from "../i18n";
import { KIND_COLOURS, formatTokens, tokenTier } from "./art/kinds";
import { lookFor, roleLook } from "./art/outfit";
import { OFFICE_COLOURS } from "./art/office";
import { recolour } from "./art/ramp";
import { SPRITES } from "./art/pixtuoid";
import {
  BOOKSHELF, DESK_GAP_X, DESK_H, DESK_W, LEAVER_MS, LEAVER_WALK_MS, PANTRY, PLANT, RUG, SIDE_TABLE, SOFA, VENDING, WALL_CLOCK,
  WALL_H, WATER_COOLER, WHITEBOARD, WINDOWS, deskPosition, floorSize, loungeLeft, loungePath, pathOffsets, podCount, seatPosition,
  sidePosition, type Point,
} from "./floorLayout";
import { PixelSprite } from "./PixelSprite";
import { usePageHidden } from "./usePageHidden";
import type { DeskSeat, FloorModel } from "./useOffice";
import type { Leaver as LeaverModel } from "./floorLayout";
import { accessibleName, caption, roleLabel, type DeskModel } from "./layout";
import "./Floor.css";

export interface FloorProps {
  floor: FloorModel;
  online: boolean;
  onOpenSession(sessionId: string): void;
  /** Extra controls at the right of the header (the Oficina/Flujo toggle). */
  headerExtra?: React.ReactNode;
  className?: string;
}

const SCALES = [5, 4, 3];
const FALLBACK_SCALE = 4;
const KINDS: { kind: ActionKind; name: string }[] = [
  { kind: "edit", name: "Edición" },
  { kind: "read", name: "Lectura" },
  { kind: "run", name: "Comando" },
  { kind: "delegate", name: "Delegación" },
  { kind: "other", name: "Otra" },
];
/** How long a starting lead takes to walk from the aisle to its chair. */
const WALK_IN_MS = 1400;
const STATE_WORD = { starting: "Arrancando", working: "Trabajando", waiting: "Esperándote", paused: "En pausa" } as const;

const reducedMotion = (): boolean => typeof window.matchMedia === "function" && window.matchMedia("(prefers-reduced-motion: reduce)").matches;

/** The largest integer scale (5, 4 or 3) at which `pods` fit the container; 4 until measured. */
function useScale(pods: number) {
  const ref = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);
  useEffect(() => {
    const el = ref.current;
    if (!el || typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(() => setWidth(el.clientWidth));
    ro.observe(el);
    setWidth(el.clientWidth);
    return () => ro.disconnect();
  }, []);
  const need = floorSize(pods).width;
  const scale = width > 0 ? (SCALES.find((s) => need * s <= width) ?? SCALES[SCALES.length - 1]) : FALLBACK_SCALE;
  return { ref, scale };
}

const px = (n: number, scale: number): string => `${n * scale}px`;
const at = (p: Point, scale: number): CSSProperties => ({ left: px(p.x, scale), top: px(p.y, scale) });

/** The glass's colour: the action's kind; a paused desk's monitor is off whatever the last action was. */
function glowOf(seat: DeskSeat): string | null {
  return seat.action && seat.state !== "paused" ? KIND_COLOURS[seat.action.kind] : null;
}

interface DeskProps {
  seat: DeskSeat;
  scale: number;
  look: ReturnType<typeof lookFor>;
  label?: string;
  name: string;
  onOpen?: () => void;
}

/** A sprite's top-left for a feet anchor: the characters are 8 px wide and at most 12 tall. */
const feetStyle = (p: Point, scale: number): CSSProperties => at({ x: p.x - 4, y: p.y - 12 }, scale);
const feetTransform = (p: Point, scale: number): string => `translate(${(p.x - 4) * scale}px, ${(p.y - 12) * scale}px)`;

/** One desk: the character behind it (its head shows above the monitor), the desk with its glass,
 *  tower and cup in front, bubble and caption. */
function Desk({ seat, scale, look, label, name, onOpen }: DeskProps) {
  const d = deskPosition(seat.slot);
  const s = seatPosition(seat.slot);
  const glow = glowOf(seat);
  const tier = seat.tokens !== undefined ? tokenTier(seat.tokens) : 0;
  const desk = SPRITES.desk;
  const cap = seat.state === "waiting" || seat.state === "paused" ? null : caption(seat.action);
  // A starting lead walks in from beside its desk to the chair, then sits; with reduced motion it just sits.
  const [walk, setWalk] = useState<"walking" | "seated">(() => (seat.state === "starting" && !reducedMotion() ? "walking" : "seated"));
  const charRef = useRef<HTMLElement>(null);
  useEffect(() => {
    if (walk === "seated") return;
    // The position is a Web Animation (it ends at the chair by itself, whatever the timers do);
    // the timer only swaps the sprite, and is the fallback where there is no Web Animations.
    const done = setTimeout(() => setWalk("seated"), WALK_IN_MS + 100);
    const el = charRef.current;
    const from = sidePosition(seat.slot);
    const anim = typeof el?.animate === "function"
      ? el.animate([{ transform: `translate(${(from.x - s.x) * scale}px, 0)` }, { transform: "translate(0, 0)" }], { duration: WALK_IN_MS, easing: "linear" })
      : undefined;
    if (anim) anim.onfinish = () => setWalk("seated");
    return () => {
      clearTimeout(done);
      anim?.cancel();
    };
  }, [walk, scale, seat.slot, s.x]);
  const charProps =
    walk === "walking"
      ? { animation: "walking" }
      : seat.state === "paused"
        ? { sprite: "seated_sleeping" }
        : seat.state === "working" && seat.action
          ? { animation: "typing" }
          : { sprite: "seated" };
  const character = <PixelSprite {...(charProps as { sprite: string })} scale={scale} recolour={recolour(look)} />;
  const charStyle = feetStyle(s, scale);
  return (
    <div className={`fl-desk fl-desk--${seat.state}`} data-slot={seat.slot} data-state={seat.state} data-kind={glow ? seat.action!.kind : ""}>
      {seat.state === "waiting" && (
        <span className="fl-bar" style={{ ...at({ x: d.x - 2, y: d.y }, scale), height: px(DESK_H, scale) }} aria-hidden="true" />
      )}
      {onOpen ? (
        <button type="button" ref={charRef as React.RefObject<HTMLButtonElement>} className="fl-char" data-walk={walk} style={charStyle} aria-label={name} onClick={onOpen}>
          {character}
        </button>
      ) : (
        <div ref={charRef as React.RefObject<HTMLDivElement>} className="fl-char fl-char--helper" data-walk={walk} style={charStyle} role="img" aria-label={name}>
          {character}
        </div>
      )}
      {glow && <span className="fl-halo" data-glow={glow} style={{ ...at({ x: d.x + 3, y: d.y + 1 }, scale), width: px(8, scale), height: px(4, scale), "--glow": `0 0 ${scale * 3}px ${scale}px ${glow}66` } as CSSProperties} aria-hidden="true" />}
      <PixelSprite sprite="desk" scale={scale} className="fl-abs" recolour={glow ? { j: glow, J: glow } : undefined} style={at(d, scale)} />
      <PixelSprite sprite="desk_cup" scale={scale} className="fl-abs" style={at({ x: d.x + desk.marks.cup[0], y: d.y + desk.marks.cup[1] }, scale)} />
      {tier > 0 && (
        <span className="fl-tower" data-tier={tier} title={`${formatTokens(seat.tokens!)} tokens`} style={at({ x: d.x + desk.marks.tower[0], y: d.y + desk.marks.tower[1] }, scale)}>
          <PixelSprite sprite="token_tower" frame={tier - 1} scale={scale} />
        </span>
      )}
      {seat.state === "waiting" && (
        <span className="fl-bubble fl-bubble--wait" style={at({ x: s.x + 4, y: s.y - 17 }, scale)} aria-hidden="true">?</span>
      )}
      {seat.state === "paused" && (
        <span className="fl-bubble fl-bubble--zzz" style={at({ x: s.x + 4, y: s.y - 17 }, scale)} aria-hidden="true">zzz</span>
      )}
      <div className="fl-label" style={{ ...at({ x: d.x - DESK_GAP_X / 2, y: d.y + DESK_H + 1 }, scale), width: px(DESK_W + DESK_GAP_X, scale) }}>
        {cap && <span className="fl-caption" title={cap}>{cap}</span>}
        {label && <span className="fl-role">{label}</span>}
      </div>
    </div>
  );
}

/** A helper that left: walks from its desk to a lounge seat and stays there; the data drops it
 *  after LEAVER_MS. Where it stands is its lounge seat from the start; the walk is a Web Animation
 *  over that (continued from `since`, so a remount does not restart it), so it can only end there. */
function Leaver({ leaver, scale, pods, still }: { leaver: LeaverModel; scale: number; pods: number; still: boolean }) {
  const { slot } = leaver;
  const path = loungePath(slot, pods);
  const dest = path[path.length - 1];
  const ref = useRef<HTMLDivElement>(null);
  const [arrived, setArrived] = useState(() => still || Date.now() - leaver.since >= LEAVER_WALK_MS);
  useEffect(() => {
    if (arrived) return;
    const left = Math.max(0, LEAVER_WALK_MS - (Date.now() - leaver.since));
    const done = setTimeout(() => setArrived(true), left);
    const el = ref.current;
    let anim: Animation | undefined;
    if (typeof el?.animate === "function") {
      const offsets = pathOffsets(path);
      anim = el.animate(path.map((p, i) => ({ transform: feetTransform(p, scale), offset: offsets[i] })), { duration: LEAVER_WALK_MS, easing: "linear" });
      anim.currentTime = LEAVER_WALK_MS - left;
      anim.onfinish = () => setArrived(true);
    }
    return () => {
      clearTimeout(done);
      anim?.cancel();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [arrived, scale, slot, pods, leaver.since]);
  const style: CSSProperties = {
    transform: feetTransform(dest, scale),
    animationDelay: `-${Math.min(Math.max(0, Date.now() - leaver.since), LEAVER_MS)}ms`,
  };
  const look = recolour(roleLook(leaver.role ?? ""));
  return (
    <div ref={ref} className="fl-leaver" data-step={still ? "still" : arrived ? "arrived" : "walking"} style={style} role="img" aria-label={t("{role}, se va", { role: roleLabel(leaver.role ?? "") })}>
      {arrived ? <PixelSprite sprite="standing" scale={scale} recolour={look} /> : <PixelSprite animation="walking" scale={scale} recolour={look} />}
    </div>
  );
}

/** The room itself, full width of the body: the north wall with its skirting, and the wooden floor
 *  (a small plank pattern). Art colours, the same in both themes. */
function Backdrop({ scale }: { scale: number }) {
  const id = useId().replace(/[^a-zA-Z0-9]/g, "");
  const c = OFFICE_COLOURS;
  const u = (n: number) => n * scale;
  return (
    <svg className="fl-bg" width="100%" height="100%" aria-hidden="true" focusable="false" shapeRendering="crispEdges">
      <defs>
        <pattern id={id} width={u(16)} height={u(8)} patternUnits="userSpaceOnUse">
          <rect width={u(16)} height={u(8)} fill={c.floor} />
          <rect width={u(16)} height={u(1)} fill={c.floorDark} />
          <rect y={u(4)} width={u(16)} height={u(1)} fill={c.floorDark} />
          <rect y={u(1)} width={u(1)} height={u(3)} fill={c.floorDark} />
          <rect x={u(8)} y={u(5)} width={u(1)} height={u(3)} fill={c.floorDark} />
          <rect x={u(10)} y={u(2)} width={u(3)} height={u(1)} fill={c.floorLight} />
          <rect x={u(3)} y={u(6)} width={u(3)} height={u(1)} fill={c.floorLight} />
        </pattern>
      </defs>
      <rect width="100%" height={u(WALL_H)} fill={c.wall} />
      <rect y={u(WALL_H - 2)} width="100%" height={u(2)} fill={c.wallTrim} />
      <rect y={u(WALL_H - 3)} width="100%" height={u(1)} fill={c.wallShade} />
      <rect y={u(WALL_H)} width="100%" height="100%" fill={`url(#${id})`} />
      <rect y={u(WALL_H)} width="100%" height={u(1)} fill={c.floorDark} />
    </svg>
  );
}

/** Windows on the wall and the lounge's rug, in art pixels of the floor box. */
function Decor({ scale, size, lounge }: { scale: number; size: { width: number; height: number }; lounge: number }) {
  const c = OFFICE_COLOURS;
  return (
    <svg className="fl-abs fl-deco" width={size.width * scale} height={size.height * scale} viewBox={`0 0 ${size.width} ${size.height}`} aria-hidden="true" focusable="false" shapeRendering="crispEdges">
      {WINDOWS.map((w) => (
        <g key={w.x}>
          <rect x={w.x - 1} y={w.y + w.h} width={w.w + 2} height={1} fill={c.wallTrim} />
          <rect x={w.x} y={w.y} width={w.w} height={w.h} fill={c.windowFrame} />
          <rect x={w.x + 1} y={w.y + 1} width={w.w / 2 - 1.5} height={w.h - 2} fill={c.windowGlass} />
          <rect x={w.x + w.w / 2 + 0.5} y={w.y + 1} width={w.w / 2 - 1.5} height={w.h - 2} fill={c.windowGlass} />
          <rect x={w.x + 1} y={w.y + 1} width={3} height={1} fill={c.windowShine} />
          <rect x={w.x + w.w / 2 + 0.5} y={w.y + 1} width={3} height={1} fill={c.windowShine} />
        </g>
      ))}
      <rect x={lounge + RUG.x} y={RUG.y} width={RUG.w} height={RUG.h} fill={c.rugEdge} />
      <rect x={lounge + RUG.x + 1} y={RUG.y + 1} width={RUG.w - 2} height={RUG.h - 2} fill={c.rug} />
    </svg>
  );
}

/** Everything standing on the wall or in the lounge, and the wall decor. */
function Furniture({ scale, pods }: { scale: number; pods: number }) {
  const lx = loungeLeft(pods);
  const pos = (r: { x: number; y: number }, dx = 0) => at({ x: dx + r.x, y: r.y }, scale);
  return (
    <>
      <PixelSprite sprite="whiteboard" scale={scale} className="fl-abs" style={pos(WHITEBOARD)} />
      <PixelSprite sprite="bookshelf" scale={scale} className="fl-abs" style={pos(BOOKSHELF)} />
      <PixelSprite sprite="wall_clock" scale={scale} className="fl-abs" style={pos(WALL_CLOCK)} />
      <PixelSprite sprite="vending_machine" scale={scale} className="fl-abs" style={pos(VENDING, lx)} />
      <PixelSprite sprite="pantry_small" scale={scale} className="fl-abs" style={pos(PANTRY, lx)} />
      <PixelSprite sprite="water_cooler" scale={scale} className="fl-abs" style={pos(WATER_COOLER, lx)} />
      <PixelSprite sprite="meeting_sofa" scale={scale} className="fl-abs" style={pos(SOFA, lx)} />
      <PixelSprite sprite="side_table" scale={scale} className="fl-abs" style={pos(SIDE_TABLE, lx)} />
      <PixelSprite sprite="plant" scale={scale} className="fl-abs" style={pos(PLANT, lx)} />
    </>
  );
}

/** One owner's office floor: framed, with its desks, helpers, leavers and lounge. */
export function Floor({ floor, online, onOpenSession, headerExtra, className }: FloorProps) {
  const hidden = usePageHidden();
  const slots = [...floor.leads, ...floor.helpers, ...floor.leaving].map((d) => d.slot);
  const pods = podCount(Math.max(-1, ...slots) + 1);
  const { ref, scale } = useScale(pods);
  const size = floorSize(pods);
  const still = reducedMotion();
  const empty = floor.leads.length === 0 && floor.helpers.length === 0;
  const cls = ["fl", !online && "fl--stale", hidden && "pg-paused", className].filter(Boolean).join(" ");
  return (
    <section className={cls} aria-label={floor.owner}>
      <header className="fl-head">
        <span className="fl-title">
          <b>{floor.owner}</b> {floor.title !== floor.owner && floor.title}
        </span>
        <span className="fl-legend">
          {KINDS.map((k) => (
            <span key={k.kind} className="fl-key">
              <i style={{ "--tint": KIND_COLOURS[k.kind] } as CSSProperties} />
              {t(k.name)}
            </span>
          ))}
        </span>
        <span className="fl-spacer" />
        {floor.waiting > 0 && <span className="fl-pill">{t("{n} esperando", { n: floor.waiting })}</span>}
        {headerExtra}
      </header>
      {!online && (
        <div className="fl-banner" role="status">
          {t("Reconectando…")}
        </div>
      )}
      <div className="fl-body" ref={ref}>
        <Backdrop scale={scale} />
        {empty && <p className="fl-empty">{floor.owner.startsWith("@") ? t("Ningún agente trabajando en este proyecto") : t("Ningún agente trabajando en esta tarea")}</p>}
        <div className="fl-floor" style={{ width: px(size.width, scale), height: px(size.height, scale) }} data-scale={scale}>
          <Decor scale={scale} size={size} lounge={loungeLeft(pods)} />
          <Furniture scale={scale} pods={pods} />
          {floor.leads.map((seat) => {
            const desk = { key: floor.owner, session: { state: seat.state === "working" ? "running" : seat.state } as unknown as Session } as DeskModel;
            const activity = seat.action ? ({ action: seat.action, helpers: [] } as Activity) : undefined;
            return (
              <Desk
                key={seat.session}
                seat={seat}
                scale={scale}
                look={lookFor(seat.lookKey, seat.session!)}
                name={accessibleName(desk, activity, seat.tokens)}
                onOpen={() => onOpenSession(seat.session!)}
              />
            );
          })}
          {floor.helpers.map((seat) => {
            const label = roleLabel(seat.lookKey);
            const cap = caption(seat.action);
            return (
              <Desk
                key={seat.helperId}
                seat={seat}
                scale={scale}
                look={roleLook(seat.lookKey)}
                label={label}
                name={`${label}, ${t(STATE_WORD[seat.state])}${cap ? `, ${cap}` : ""}`}
              />
            );
          })}
          {floor.leaving.map((l) => (
            <Leaver key={l.id} leaver={l} scale={scale} pods={pods} still={still} />
          ))}
        </div>
      </div>
    </section>
  );
}
