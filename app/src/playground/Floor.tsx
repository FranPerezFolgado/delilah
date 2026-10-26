import { useEffect, useRef, useState, type CSSProperties } from "react";
import type { ActionKind, Activity, Session } from "../api";
import { t } from "../i18n";
import { KIND_COLOURS } from "./art/kinds";
import { lookFor, roleLook } from "./art/outfit";
import { recolour } from "./art/ramp";
import {
  DESK_GAP_X, DESK_H, DESK_W, LEAVER_MS, LEAVER_WALK_MS, floorSize, loungePath, pathOffsets, podCount, seatPosition,
  sidePosition, deskPosition, type Point,
} from "./floorLayout";
import { PixelSprite } from "./PixelSprite";
import { DeskArt, SceneArt, Backdrop, at, feetStyle, px, seatedProps } from "./FloorScene";
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


interface DeskProps {
  seat: DeskSeat;
  scale: number;
  look: ReturnType<typeof lookFor>;
  label?: string;
  name: string;
  onOpen?: () => void;
}

const feetTransform = (p: Point, scale: number): string => `translate(${(p.x - 4) * scale}px, ${(p.y - 12) * scale}px)`;

/** One desk: the character behind it (its head shows above the monitor), the desk with its glass,
 *  tower and cup in front (DeskArt), bubble and caption. */
function Desk({ seat, scale, look, label, name, onOpen }: DeskProps) {
  const d = deskPosition(seat.slot);
  const s = seatPosition(seat.slot);
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
  const charProps = walk === "walking" ? { animation: "walking" } : seatedProps(seat);
  const character = <PixelSprite {...(charProps as { sprite: string })} scale={scale} recolour={recolour(look)} />;
  const charStyle = feetStyle(s, scale);
  return (
    <DeskArt
      seat={seat}
      scale={scale}
      after={
        <div className="fl-label" style={{ ...at({ x: d.x - DESK_GAP_X / 2, y: d.y + DESK_H + 1 }, scale), width: px(DESK_W + DESK_GAP_X, scale) }}>
          {cap && <span className="fl-caption" title={cap}>{cap}</span>}
          {label && <span className="fl-role">{label}</span>}
        </div>
      }
    >
      {onOpen ? (
        <button type="button" ref={charRef as React.RefObject<HTMLButtonElement>} className="fl-char" data-walk={walk} style={charStyle} aria-label={name} onClick={onOpen}>
          {character}
        </button>
      ) : (
        <div ref={charRef as React.RefObject<HTMLDivElement>} className="fl-char fl-char--helper" data-walk={walk} style={charStyle} role="img" aria-label={name}>
          {character}
        </div>
      )}
    </DeskArt>
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
          <SceneArt scale={scale} pods={pods} />
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
