import type { CSSProperties, ReactElement } from "react";

/** A pixel map as frames of rows: one character per pixel, `.` transparent, any other
 *  letter a palette key painted with `var(--pg-<key>)`. The frames sit side by side in
 *  one group; CSS `steps(var(--pg-frames))` moves the group to cycle them. */
export function Sprite({ frames, className }: { frames: string[][]; className?: string }) {
  const w = Math.max(...frames.map((f) => Math.max(...f.map((r) => r.length))));
  const h = Math.max(...frames.map((f) => f.length));
  const rects: ReactElement[] = [];
  frames.forEach((rows, i) => {
    rows.forEach((row, y) => {
      for (let x = 0; x < row.length; ) {
        const key = row[x];
        let end = x + 1;
        while (end < row.length && row[end] === key) end++;
        if (key !== ".") {
          rects.push(<rect key={`${i}-${y}-${x}`} x={i * w + x} y={y} width={end - x} height={1} fill={`var(--pg-${key})`} />);
        }
        x = end;
      }
    });
  });
  const style = { "--pg-frames": frames.length, "--pg-w": `${w}px` } as CSSProperties;
  return (
    <svg
      className={className}
      style={style}
      viewBox={`0 0 ${w} ${h}`}
      shapeRendering="crispEdges"
      aria-hidden="true"
    >
      <g className={frames.length > 1 ? "pg-anim" : undefined}>{rects}</g>
    </svg>
  );
}
