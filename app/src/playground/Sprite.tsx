import { memo, type CSSProperties } from "react";

interface Parsed {
  w: number;
  h: number;
  /** Per frame, the path data of each colour key. */
  paths: { frame: number; key: string; d: string }[];
}

const cache = new WeakMap<string[][], Parsed>();

function parse(frames: string[][]): Parsed {
  const hit = cache.get(frames);
  if (hit) return hit;
  const w = Math.max(...frames.map((f) => Math.max(...f.map((r) => r.length))));
  const h = Math.max(...frames.map((f) => f.length));
  const paths: Parsed["paths"] = [];
  frames.forEach((rows, i) => {
    const byKey = new Map<string, string>();
    rows.forEach((row, y) => {
      for (let x = 0; x < row.length; ) {
        const key = row[x];
        let end = x + 1;
        while (end < row.length && row[end] === key) end++;
        if (key !== ".") byKey.set(key, `${byKey.get(key) ?? ""}M${i * w + x} ${y}h${end - x}v1h-${end - x}z`);
        x = end;
      }
    });
    for (const [key, d] of byKey) paths.push({ frame: i, key, d });
  });
  const parsed = { w, h, paths };
  cache.set(frames, parsed);
  return parsed;
}

/** A pixel map as frames of rows: one character per pixel, `.` transparent, any other
 *  letter a palette key painted with `var(--pg-<key>)` (single letters are reserved for
 *  colours). The frames sit side by side in one group; CSS `steps(var(--pg-fn))` moves
 *  the group to cycle them. */
export const Sprite = memo(function Sprite({ frames, className }: { frames: string[][]; className?: string }) {
  const { w, h, paths } = parse(frames);
  const style = { "--pg-fn": frames.length, "--pg-fw": `${w}px` } as CSSProperties;
  return (
    <svg className={className} style={style} viewBox={`0 0 ${w} ${h}`} shapeRendering="crispEdges" aria-hidden="true">
      <g className={frames.length > 1 ? "pg-anim" : undefined}>
        {paths.map((p) => (
          <path key={`${p.frame}-${p.key}`} d={p.d} fill={`var(--pg-${p.key})`} />
        ))}
      </g>
    </svg>
  );
});
