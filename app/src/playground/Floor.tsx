import { useEffect, useRef, useState, type CSSProperties } from "react";
import type { Action, ActionKind, Activity, Session } from "../api";
import { t } from "../i18n";
import { KIND_COLOURS, formatTokens, tokenTier } from "./art/kinds";
import { lookFor, roleLook as roleOutfit } from "./art/outfit";
import { recolour } from "./art/ramp";
import { SPRITES } from "./art/pixtuoid";
import {
  LOUNGE_SEATS, MARGIN, PANTRY, PLANTS, SIDE_TABLE, SOFA, WATER_COOLER, DESK_GAP_X, DESK_H, DESK_W,
  deskPosition, floorSize, loungePath, podCount, seatPosition, type Point,
} from "./floorLayout";
import { PixelSprite } from "./PixelSprite";
import type { DeskSeat, FloorModel } from "./useOffice";
import { accessibleName, caption, roleLook, type DeskModel } from "./layout";
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
const STATE_WORD = { starting: "Arrancando", working: "Trabajando", waiting: "Esperándote", paused: "En pausa" } as const;

/** True while the page is hidden, so the floor can stop animating. */
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

function glowOf(action: Action | null): string | null {
  return action ? KIND_COLOURS[action.kind] : null;
}

interface DeskProps {
  seat: DeskSeat;
  scale: number;
  look: ReturnType<typeof lookFor>;
  label?: string;
  name: string;
  onOpen?: () => void;
}

/** One desk: chair, character, desk with its glass, tower and cup, bubble and caption. */
function Desk({ seat, scale, look, label, name, onOpen }: DeskProps) {
  const d = deskPosition(seat.slot);
  const s = seatPosition(seat.slot);
  const glow = glowOf(seat.action);
  const tier = seat.tokens !== undefined ? tokenTier(seat.tokens) : 0;
  const desk = SPRITES.desk;
  const cap = seat.state === "waiting" || seat.state === "paused" ? null : caption(seat.action);
  const charProps =
    seat.state === "starting"
      ? { sprite: "standing" }
      : seat.state === "paused"
        ? { sprite: "seated_sleeping" }
        : seat.state === "working" && seat.action
          ? { animation: "typing" }
          : { sprite: "seated" };
  const character = <PixelSprite {...(charProps as { sprite: string })} scale={scale} recolour={recolour(look)} />;
  const charStyle: CSSProperties = at({ x: s.x - 4, y: seat.state === "starting" ? s.y - 12 : s.y - 10 }, scale);
  return (
    <div className={`fl-desk fl-desk--${seat.state}`} data-slot={seat.slot} data-state={seat.state} data-kind={seat.action?.kind ?? ""}>
      {seat.state === "waiting" && (
        <span className="fl-bar" style={{ ...at({ x: d.x - 2, y: d.y }, scale), height: px(DESK_H + 6, scale) }} aria-hidden="true" />
      )}
      <PixelSprite sprite="desk_chair" scale={scale} className="fl-abs" style={at({ x: s.x - 4, y: s.y - 5 }, scale)} />
      {onOpen ? (
        <button type="button" className="fl-char" style={charStyle} aria-label={name} onClick={onOpen}>
          {character}
        </button>
      ) : (
        <div className="fl-char fl-char--helper" style={charStyle} role="img" aria-label={name}>
          {character}
        </div>
      )}
      {glow && <span className="fl-halo" data-glow={glow} style={{ ...at({ x: d.x + 3, y: d.y + 1 }, scale), width: px(8, scale), height: px(4, scale), boxShadow: `0 0 ${scale * 3}px ${scale}px ${glow}66` }} aria-hidden="true" />}
      <PixelSprite sprite="desk" scale={scale} className="fl-abs" recolour={glow ? { j: glow, J: glow } : undefined} style={at(d, scale)} />
      <PixelSprite sprite="desk_cup" scale={scale} className="fl-abs" style={at({ x: d.x + desk.marks.cup[0], y: d.y + desk.marks.cup[1] }, scale)} />
      {tier > 0 && (
        <span className="fl-tower" data-tier={tier} title={`${formatTokens(seat.tokens!)} tokens`} style={at({ x: d.x + desk.marks.tower[0], y: d.y + desk.marks.tower[1] }, scale)}>
          <PixelSprite sprite="token_tower" frame={tier - 1} scale={scale} />
        </span>
      )}
      {seat.state === "waiting" && (
        <span className="fl-bubble fl-bubble--wait" style={at({ x: s.x + 3, y: s.y - 16 }, scale)} aria-hidden="true">?</span>
      )}
      {seat.state === "paused" && (
        <span className="fl-bubble fl-bubble--zzz" style={at({ x: s.x + 3, y: s.y - 16 }, scale)} aria-hidden="true">zzz</span>
      )}
      <div className="fl-label" style={{ ...at({ x: d.x - DESK_GAP_X / 2, y: s.y + 1 }, scale), width: px(DESK_W + DESK_GAP_X, scale) }}>
        {cap && <span className="fl-caption" title={cap}>{cap}</span>}
        {label && <span className="fl-role">{label}</span>}
      </div>
    </div>
  );
}

/** A helper that left: walks desk, aisle, lounge seat; the data drops it after LEAVER_MS. */
function Leaver({ slot, scale, still }: { slot: number; scale: number; still: boolean }) {
  const path = loungePath(slot);
  const [step, setStep] = useState(0);
  useEffect(() => {
    if (still) return;
    const ids = [setTimeout(() => setStep(1), 30), setTimeout(() => setStep(2), 1400), setTimeout(() => setStep(3), 3200)];
    return () => ids.forEach(clearTimeout);
  }, [still]);
  const p = still ? LOUNGE_SEATS[slot % LOUNGE_SEATS.length] : path[Math.min(step, 2)];
  const style: CSSProperties = {
    transform: `translate(${px(p.x - 4, scale)}, ${px(p.y - 12, scale)})`,
    transition: step === 1 ? "transform 1.3s linear" : step === 2 ? "transform 1.7s linear" : "none",
  };
  const walking = !still && step < 3;
  return (
    <div className="fl-leaver" data-step={still ? "still" : step} style={style}>
      {walking ? <PixelSprite animation="walking" scale={scale} /> : <PixelSprite sprite="standing" scale={scale} />}
    </div>
  );
}

function Lounge({ scale }: { scale: number }) {
  const o: Point = { x: LOUNGE_SEATS[0].x - 6, y: MARGIN };
  const pos = (r: { x: number; y: number }) => at({ x: o.x + r.x, y: o.y + r.y }, scale);
  return (
    <>
      <PixelSprite sprite="pantry_small" scale={scale} className="fl-abs" style={pos(PANTRY)} />
      <PixelSprite sprite="water_cooler" scale={scale} className="fl-abs" style={pos(WATER_COOLER)} />
      <PixelSprite sprite="meeting_sofa" scale={scale} className="fl-abs" style={pos(SOFA)} />
      <PixelSprite sprite="side_table" scale={scale} className="fl-abs" style={pos(SIDE_TABLE)} />
      {PLANTS.map((r, i) => (
        <PixelSprite key={i} sprite={r.h > 7 ? "plant_tall" : "plant"} scale={scale} className="fl-abs" style={pos(r)} />
      ))}
    </>
  );
}

/** One owner's office floor: framed, with its desks, helpers, leavers and lounge. */
export function Floor({ floor, online, onOpenSession, headerExtra, className }: FloorProps) {
  const hidden = useHidden();
  const slots = [...floor.leads, ...floor.helpers, ...floor.leaving].map((d) => d.slot);
  const { ref, scale } = useScale(podCount(Math.max(-1, ...slots) + 1));
  const size = floorSize(podCount(Math.max(-1, ...slots) + 1));
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
              <i style={{ background: KIND_COLOURS[k.kind] }} />
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
        {empty && <p className="fl-empty">{t("Ningún agente trabajando en esta tarea")}</p>}
        <div className="fl-floor" style={{ width: px(size.width, scale), height: px(size.height, scale) }} data-scale={scale}>
          <Lounge scale={scale} />
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
            const { label } = roleLook(seat.lookKey);
            const cap = caption(seat.action);
            return (
              <Desk
                key={seat.helperId}
                seat={seat}
                scale={scale}
                look={roleOutfit(seat.lookKey)}
                label={label}
                name={`${label}, ${t(STATE_WORD[seat.state])}${cap ? `, ${cap}` : ""}`}
              />
            );
          })}
          {floor.leaving.map((l) => (
            <Leaver key={l.id} slot={l.slot} scale={scale} still={still} />
          ))}
        </div>
      </div>
    </section>
  );
}
