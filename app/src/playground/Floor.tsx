import { useEffect, useRef, useState, type CSSProperties } from "react";
import type { ActionKind, Activity, Session } from "../api";
import { t } from "../i18n";
import { KIND_COLOURS, formatTokens, tokenTier } from "./art/kinds";
import { lookFor, roleLook } from "./art/outfit";
import { recolour } from "./art/ramp";
import { SPRITES } from "./art/pixtuoid";
import {
  LOUNGE_SEATS, MARGIN, PANTRY, PLANTS, SIDE_TABLE, SOFA, WATER_COOLER, DESK_GAP_X, DESK_H, DESK_W,
  deskPosition, floorSize, loungePath, podCount, seatPosition, type Point,
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

/** One desk: chair, character, desk with its glass, tower and cup, bubble and caption. */
function Desk({ seat, scale, look, label, name, onOpen }: DeskProps) {
  const d = deskPosition(seat.slot);
  const s = seatPosition(seat.slot);
  const glow = glowOf(seat);
  const tier = seat.tokens !== undefined ? tokenTier(seat.tokens) : 0;
  const desk = SPRITES.desk;
  const cap = seat.state === "waiting" || seat.state === "paused" ? null : caption(seat.action);
  // A starting lead walks in from the aisle to its chair, then sits; with reduced motion it just sits.
  const [walk, setWalk] = useState<"from" | "walking" | "seated">(() => (seat.state === "starting" && !reducedMotion() ? "from" : "seated"));
  const walkIn = useRef(walk === "from");
  useEffect(() => {
    if (!walkIn.current) return;
    const ids = [setTimeout(() => setWalk("walking"), 30), setTimeout(() => setWalk("seated"), WALK_IN_MS + 100)];
    return () => ids.forEach(clearTimeout);
  }, []);
  const walking = walk !== "seated";
  const charProps = walking
    ? { animation: "walking" }
    : seat.state === "paused"
      ? { sprite: "seated_sleeping" }
      : seat.state === "working" && seat.action
        ? { animation: "typing" }
        : { sprite: "seated" };
  const character = <PixelSprite {...(charProps as { sprite: string })} scale={scale} recolour={recolour(look)} />;
  const charStyle: CSSProperties = {
    ...at({ x: s.x - 4, y: walking ? s.y - 12 : s.y - 10 }, scale),
    transform: walk === "from" ? `translate(0, ${px(loungePath(seat.slot)[1].y - s.y, scale)})` : undefined,
    transition: walk === "walking" ? `transform ${WALK_IN_MS}ms linear` : undefined,
  };
  return (
    <div className={`fl-desk fl-desk--${seat.state}`} data-slot={seat.slot} data-state={seat.state} data-kind={glow ? seat.action!.kind : ""}>
      {seat.state === "waiting" && (
        <span className="fl-bar" style={{ ...at({ x: d.x - 2, y: d.y }, scale), height: px(DESK_H + 6, scale) }} aria-hidden="true" />
      )}
      <PixelSprite sprite="desk_chair" scale={scale} className="fl-abs" style={at({ x: s.x - 4, y: s.y - 5 }, scale)} />
      {onOpen ? (
        <button type="button" className="fl-char" data-walk={walk} style={charStyle} aria-label={name} onClick={onOpen}>
          {character}
        </button>
      ) : (
        <div className="fl-char fl-char--helper" data-walk={walk} style={charStyle} role="img" aria-label={name}>
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

/** A helper that left: walks desk, aisle, lounge seat and stays there; the data drops it after LEAVER_MS. */
function Leaver({ leaver, scale, still }: { leaver: LeaverModel; scale: number; still: boolean }) {
  const { slot } = leaver;
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
  const look = recolour(roleLook(leaver.role ?? ""));
  return (
    <div className="fl-leaver" data-step={still ? "still" : step} style={style} role="img" aria-label={t("{role}, se va", { role: roleLabel(leaver.role ?? "") })}>
      {walking ? <PixelSprite animation="walking" scale={scale} recolour={look} /> : <PixelSprite sprite="standing" scale={scale} recolour={look} />}
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
  const hidden = usePageHidden();
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
        {empty && <p className="fl-empty">{floor.owner.startsWith("@") ? t("Ningún agente trabajando en este proyecto") : t("Ningún agente trabajando en esta tarea")}</p>}
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
            <Leaver key={l.id} leaver={l} scale={scale} still={still} />
          ))}
        </div>
      </div>
    </section>
  );
}
