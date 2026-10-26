import type { CSSProperties } from "react";
import { ANIMATIONS, PALETTE, RAMPS, SPRITES } from "./art/pixtuoid";
import { ramp } from "./art/ramp";
import "./PixelSprite.css";

type Source = { sprite: string; animation?: undefined } | { animation: string; sprite?: undefined };

export type PixelSpriteProps = Source & {
  /** Static sprite frame index (default 0). */
  frame?: number;
  /** CSS px per art px. */
  scale: number;
  /** Palette key to hex overrides, applied on top of PALETTE. */
  recolour?: Record<string, string>;
  className?: string;
  style?: CSSProperties;
};

export interface SpritePath {
  fill: string;
  d: string;
}

/** One path per distinct colour; horizontal runs of the same colour are merged per row. */
export function spritePaths(rows: string[], palette: Record<string, string>, yOffset = 0): SpritePath[] {
  const byColour = new Map<string, string>();
  rows.forEach((row, y) => {
    let x = 0;
    while (x < row.length) {
      const colour = row[x] === "." ? undefined : palette[row[x]];
      let end = x + 1;
      while (end < row.length && (row[end] === "." ? undefined : palette[row[end]]) === colour) end++;
      if (colour) {
        const len = end - x;
        byColour.set(colour, (byColour.get(colour) ?? "") + `M${x} ${y + yOffset}h${len}v1h-${len}z`);
      }
      x = end;
    }
  });
  return [...byColour].map(([fill, d]) => ({ fill, d }));
}

/** PALETTE plus every derived ramp key, from the default colours. */
const BASE: Record<string, string> = { ...PALETTE };
for (const [key, def] of Object.entries(RAMPS)) if (PALETTE[def.of]) BASE[key] = ramp(PALETTE[def.of], def.level);

const cache = new Map<string, SpritePath[]>();

function cachedPaths(key: string, rows: string[], palette: Record<string, string>, yOffset: number): SpritePath[] {
  let hit = cache.get(key);
  if (!hit) {
    hit = spritePaths(rows, palette, yOffset);
    cache.set(key, hit);
  }
  return hit;
}

export function PixelSprite(props: PixelSpriteProps) {
  const { scale, recolour, className, style } = props;
  const palette = recolour ? { ...BASE, ...recolour } : BASE;
  const rc = recolour ? JSON.stringify(recolour) : "";

  // Frames to draw: [spriteName, frameIndex] pairs; shorter frames sit at the bottom.
  let refs: [string, number][];
  let frameMs = 0;
  if (props.animation !== undefined) {
    const anim = ANIMATIONS[props.animation];
    frameMs = anim.frames.length > 0 ? anim.frameMs : 0;
    refs = anim.frames.flatMap((n) => SPRITES[n].frames.map((_, i): [string, number] => [n, i]));
  } else {
    refs = [[props.sprite, props.frame ?? 0]];
  }
  const animated = props.animation !== undefined && refs.length > 1;
  const first = SPRITES[refs[0][0]];
  const w = Math.max(...refs.map(([n]) => SPRITES[n].w), first.w);
  const h = Math.max(...refs.map(([n]) => SPRITES[n].h));

  const groups = refs.map(([name, fi], i) => {
    const rows = SPRITES[name].frames[fi];
    const paths = cachedPaths(`${name}#${fi}#${h}#${rc}`, rows, palette, h - rows.length).map((p) => (
      <path key={p.fill} fill={p.fill} d={p.d} />
    ));
    if (!animated) return <g key={i}>{paths}</g>;
    const gStyle = {
      "--pg-dur": `${refs.length * frameMs}ms`,
      "--pg-delay": i === 0 ? "0ms" : `-${i * frameMs}ms`,
      animationName: `pg-frame-${Math.min(refs.length, 16)}`,
    } as CSSProperties;
    return (
      <g key={i} className="pg-frame" style={gStyle}>
        {paths}
      </g>
    );
  });

  return (
    <svg
      className={className ? `pg-sprite ${className}` : "pg-sprite"}
      style={style}
      width={w * scale}
      height={h * scale}
      viewBox={`0 0 ${w} ${h}`}
      aria-hidden="true"
      focusable="false"
      shapeRendering="crispEdges"
    >
      {groups}
    </svg>
  );
}
