import { useId, type CSSProperties, type ReactNode } from "react";
import type { DeskSeat } from "./useOffice";
import { formatTokens } from "../format";
import { KIND_COLOURS, tokenTier } from "./art/kinds";
import { OFFICE_COLOURS } from "./art/office";
import { SPRITES } from "./art/pixtuoid";
import {
  BOOKSHELF, DESK_H, PANTRY, PLANT, RUG, SIDE_TABLE, SOFA, VENDING, WALL_CLOCK,
  WALL_H, WATER_COOLER, WHITEBOARD, WINDOWS, deskPosition, floorSize, loungeLeft, seatPosition, type Point,
} from "./floorLayout";
import { PixelSprite } from "./PixelSprite";
import "./Floor.css";

// The pure, non-interactive pieces of an office floor, shared by the full Floor and the building's
// FloorMini so the two pictures cannot drift apart.

export const px = (n: number, scale: number): string => `${n * scale}px`;
export const at = (p: Point, scale: number): CSSProperties => ({ left: px(p.x, scale), top: px(p.y, scale) });

/** A sprite's top-left for a feet anchor: the characters are 8 px wide and at most 12 tall. */
export const feetStyle = (p: Point, scale: number): CSSProperties => at({ x: p.x - 4, y: p.y - 12 }, scale);

/** The glass's colour: the action's kind; a paused desk's monitor is off whatever the last action was. */
export function glowOf(seat: DeskSeat): string | null {
  return seat.action && seat.state !== "paused" ? KIND_COLOURS[seat.action.kind] : null;
}

/** The sprite a seated character shows for its state (the starting lead's walk-in is the Floor's own). */
export function seatedProps(seat: DeskSeat): { sprite: string } | { animation: string } {
  if (seat.state === "paused") return { sprite: "seated_sleeping" };
  if (seat.state === "working" && seat.action) return { animation: "typing" };
  return { sprite: "seated" };
}

/** One desk's furniture and marks, with the character (a child) behind it: waiting bar, monitor glow,
 *  desk, cup, token tower, and the `?` / `zzz` bubble. `after` goes last (the Floor's caption). */
export function DeskArt({ seat, scale, children, after }: { seat: DeskSeat; scale: number; children: ReactNode; after?: ReactNode }) {
  const d = deskPosition(seat.slot);
  const s = seatPosition(seat.slot);
  const glow = glowOf(seat);
  const tier = seat.tokens !== undefined ? tokenTier(seat.tokens) : 0;
  const desk = SPRITES.desk;
  return (
    <div className={`fl-desk fl-desk--${seat.state}`} data-slot={seat.slot} data-state={seat.state} data-kind={glow ? seat.action!.kind : ""}>
      {seat.state === "waiting" && (
        <span className="fl-bar" style={{ ...at({ x: d.x - 2, y: d.y }, scale), height: px(DESK_H, scale) }} aria-hidden="true" />
      )}
      {children}
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
      {after}
    </div>
  );
}

/** The room itself, full width of the body: the north wall with its skirting, and the wooden floor
 *  (a small plank pattern). Art colours, the same in both themes. */
export function Backdrop({ scale }: { scale: number }) {
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
export function Decor({ scale, size, lounge }: { scale: number; size: { width: number; height: number }; lounge: number }) {
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
export function Furniture({ scale, pods }: { scale: number; pods: number }) {
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

/** Room, wall decor and furniture for `pods` pods, in the floor box of `floorSize(pods)`. */
export function SceneArt({ scale, pods }: { scale: number; pods: number }) {
  return (
    <>
      <Decor scale={scale} size={floorSize(pods)} lounge={loungeLeft(pods)} />
      <Furniture scale={scale} pods={pods} />
    </>
  );
}
