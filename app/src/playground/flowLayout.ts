// Pure layout of a FlowModel. One column per agent/helper (the owner on top, its tool calls
// below, newest at the bottom). Columns are ordered by owner start time, so entries that arrive
// later (never earlier than what is there) only add columns on the right or rows at the bottom
// and never move a node that is already placed.
import type { FlowModel } from "./flowModel";

export const NODE_W = 180;
export const NODE_H = 36;
export const GAP_X = 40;
export const GAP_Y = 12;

export interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface FlowLayout {
  positions: Record<string, Box>;
  bounds: Box;
}

const time = (at: string) => {
  const t = Date.parse(at);
  return Number.isNaN(t) ? 0 : t;
};

export function layoutFlow(model: FlowModel): FlowLayout {
  const positions: Record<string, Box> = {};
  const owners = model.nodes
    .map((n, i) => ({ n, i }))
    .filter(({ n }) => n.type !== "tool")
    .sort((a, b) => time(a.n.start) - time(b.n.start) || a.i - b.i);
  const rows = new Map<string, number>();
  owners.forEach(({ n }, col) => {
    rows.set(n.id, 0);
    positions[n.id] = { x: col * (NODE_W + GAP_X), y: 0, w: NODE_W, h: NODE_H };
  });
  const col = new Map(owners.map(({ n }, c) => [n.id, c]));
  // Model order is the order of arrival, so the n-th tool of an owner is the n-th row.
  for (const n of model.nodes) {
    if (n.type !== "tool" || !n.parent || !col.has(n.parent)) continue;
    const r = (rows.get(n.parent) ?? 0) + 1;
    rows.set(n.parent, r);
    positions[n.id] = {
      x: col.get(n.parent)! * (NODE_W + GAP_X),
      y: r * (NODE_H + GAP_Y) + GAP_Y,
      w: NODE_W,
      h: NODE_H,
    };
  }
  const boxes = Object.values(positions);
  const bounds =
    boxes.length === 0
      ? { x: 0, y: 0, w: 0, h: 0 }
      : {
          x: 0,
          y: 0,
          w: Math.max(...boxes.map((b) => b.x + b.w)),
          h: Math.max(...boxes.map((b) => b.y + b.h)),
        };
  return { positions, bounds };
}
